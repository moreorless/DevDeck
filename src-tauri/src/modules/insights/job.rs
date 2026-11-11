//! 后台任务：一键生成报告（`claude -p "/insights"`）并翻译成中文；同一时刻只允许一个任务
//!
//! `/insights` 各部分的模型调用不写入会话日志，用量统计看不到这部分花费，
//! 因此由这里记录 CLI 返回的估算费用（`insights_report.cost_usd`）。

use std::{sync::Mutex, time::Duration};

use anyhow::{Result, anyhow};
use chrono::Utc;
use rusqlite::params;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use super::{EVT_INSIGHTS_UPDATED, archive, cli, translate};
use crate::{claude_log::scanner::fmt_ts, state::AppState};

pub const EVT_INSIGHTS_JOB: &str = "insights://job";
const GENERATE_TIMEOUT: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobStatus {
    pub running: bool,
    /// generating | translating
    pub phase: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    /// 上次任务的结果说明与错误
    pub message: Option<String>,
    pub error: Option<String>,
    /// 上次任务的估算费用（美元）
    pub cost_usd: Option<f64>,
    /// 上次任务涉及的报告
    pub report_id: Option<i64>,
}

#[derive(Default)]
pub struct InsightsJob(Mutex<JobStatus>);

impl InsightsJob {
    pub fn status(&self) -> JobStatus {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    fn update(&self, app: &AppHandle, f: impl FnOnce(&mut JobStatus)) {
        let snapshot = {
            let mut s = self.0.lock().unwrap_or_else(|e| e.into_inner());
            f(&mut s);
            s.clone()
        };
        let _ = app.emit(EVT_INSIGHTS_JOB, snapshot);
    }
}

pub enum JobKind {
    /// 生成新报告；`translate` 为真时随后翻译
    Generate {
        translate: bool,
    },
    Translate {
        id: i64,
    },
}

/// 启动后台任务；已有任务在运行时报错
pub fn start(app: &AppHandle, kind: JobKind) -> Result<()> {
    let job = app.state::<InsightsJob>();
    {
        let mut s = job.0.lock().unwrap_or_else(|e| e.into_inner());
        if s.running {
            return Err(anyhow!("已有 Insights 任务在运行，请等待完成"));
        }
        let phase = match kind {
            JobKind::Generate { .. } => "generating",
            JobKind::Translate { .. } => "translating",
        };
        *s = JobStatus {
            running: true,
            phase: Some(phase.into()),
            started_at: Some(fmt_ts(&Utc::now())),
            ..Default::default()
        };
    }
    job.update(app, |_| {});

    let app = app.clone();
    std::thread::spawn(move || {
        let mut cost = 0.0;
        let mut report_id = None;
        let result = run(&app, kind, &mut cost, &mut report_id);
        let job = app.state::<InsightsJob>();
        job.update(&app, |s| {
            s.running = false;
            s.phase = None;
            s.finished_at = Some(fmt_ts(&Utc::now()));
            s.cost_usd = Some(cost);
            s.report_id = report_id;
            match result {
                Ok(msg) => s.message = Some(msg),
                Err(e) => s.error = Some(format!("{e:#}")),
            }
        });
        let _ = app.emit(EVT_INSIGHTS_UPDATED, 0);
    });
    Ok(())
}

fn run(app: &AppHandle, kind: JobKind, cost: &mut f64, report_id: &mut Option<i64>) -> Result<String> {
    let state = app.state::<AppState>();
    let cfg = state.config();
    let claude = cli::find_claude(cfg.insights.claude_path.as_deref())?;
    let cwd = state.db_path.parent().map(|p| p.to_path_buf()).unwrap_or_else(std::env::temp_dir);

    match kind {
        JobKind::Generate { translate } => {
            let out = cli::run_json(
                &claude,
                &[
                    "-p",
                    "/insights",
                    "--strict-mcp-config",
                    "--no-session-persistence",
                    "--settings",
                    cli::NO_HOOKS_SETTINGS,
                ],
                None,
                &cwd,
                GENERATE_TIMEOUT,
            )?;
            *cost += out.cost_usd.unwrap_or(0.0);
            let ids = archive::sync(&state.conn(), &cfg.candidate_dirs(), &state.insights_dir)?;
            let Some(&id) = ids.last() else {
                return Err(anyhow!("claude 已运行完成，但没有发现新报告：{}", out.result.trim()));
            };
            *report_id = Some(id);
            add_cost(&state, id, out.cost_usd.unwrap_or(0.0))?;
            let _ = app.emit(EVT_INSIGHTS_UPDATED, ids.len());
            if !translate {
                return Ok("报告已生成".into());
            }
            app.state::<InsightsJob>().update(app, |s| s.phase = Some("translating".into()));
            match translate_one(&state, &claude, &cwd, id) {
                Ok((c, left)) => {
                    *cost += c;
                    Ok(format!("报告已生成并翻译为中文{}", untranslated_note(left)))
                }
                // 生成本身已成功：翻译失败只提示，可在页面上重试
                Err(e) => Err(anyhow!("报告已生成，但翻译失败（可在报告页重试）：{e:#}")),
            }
        }
        JobKind::Translate { id } => {
            *report_id = Some(id);
            let (c, left) = translate_one(&state, &claude, &cwd, id)?;
            *cost += c;
            Ok(format!("已翻译为中文{}", untranslated_note(left)))
        }
    }
}

fn untranslated_note(left: usize) -> String {
    if left == 0 { String::new() } else { format!("（{left} 段未能翻译，保留原文）") }
}

/// 翻译并保存译文，返回（费用，保留原文的段数）
fn translate_one(state: &AppState, claude: &std::path::Path, cwd: &std::path::Path, id: i64) -> Result<(f64, usize)> {
    let src = archive::archived_path(&state.conn(), &state.insights_dir, id, false)?;
    let html = String::from_utf8_lossy(&std::fs::read(&src)?).into_owned();
    let t = translate::translate_html(claude, cwd, &html)?;
    archive::save_translation(&state.conn(), &state.insights_dir, id, &t.html)?;
    add_cost(state, id, t.cost_usd)?;
    Ok((t.cost_usd, t.untranslated))
}

fn add_cost(state: &AppState, id: i64, usd: f64) -> Result<()> {
    state
        .conn()
        .execute("UPDATE insights_report SET cost_usd = COALESCE(cost_usd, 0) + ?1 WHERE id = ?2", params![usd, id])?;
    Ok(())
}
