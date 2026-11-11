import MarkdownIt from "markdown-it";
import hljs from "highlight.js/lib/common";
import "./markdown.css";

/**
 * 会话内容的 Markdown 渲染。
 * html: false —— 原始 HTML 一律转义，会话内容不能向页面注入标签或脚本。
 */
const escapeHtml = (s: string) =>
  s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

const md = new MarkdownIt({
  html: false,
  linkify: true,
  breaks: false,
  highlight(code, lang): string {
    if (lang && hljs.getLanguage(lang)) {
      try {
        return `<pre class="hljs"><code>${hljs.highlight(code, { language: lang, ignoreIllegals: true }).value}</code></pre>`;
      } catch {
        /* 回退到纯文本 */
      }
    }
    return `<pre class="hljs"><code>${escapeHtml(code)}</code></pre>`;
  },
});

export const renderMarkdown = (src: string) => md.render(src);
