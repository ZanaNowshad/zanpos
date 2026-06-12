import React from "react";

export function stripXmlArtifacts(text: string): string {
  return text
    .replace(/<tool_call>[\s\S]*?<\/tool_call>/g, "")
    .replace(/<function=\S+>[\s\S]*?<\/function>/g, "")
    .trim();
}

export function inlineMarkdown(text: string): React.ReactNode {
  return text
    .replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>")
    .replace(/\*(.+?)\*/g, "<em>$1</em>")
    .replace(/`([^`]+)`/g, "<code>$1</code>");
}

export const MarkdownContent = React.memo(function MarkdownContent({ text }: { text: string }) {
  const nodes = React.useMemo(() => parseMarkdown(stripXmlArtifacts(text)), [text]);
  return <div className="md-body">{nodes}</div>;
});

export function parseMarkdown(text: string): React.ReactNode[] {
  const lines = text.split("\n");
  const result: React.ReactNode[] = [];
  let i = 0;
  let key = 0;

  while (i < lines.length) {
    const line = lines[i];

    if (line.trim() === "") { i++; continue; }

    if (line.trimStart().startsWith("```")) {
      const lang = line.trimStart().slice(3).trim();
      const codeLines: string[] = [];
      i++;
      while (i < lines.length && !lines[i].trimStart().startsWith("```")) {
        codeLines.push(lines[i]); i++;
      }
      result.push(<div key={key++} className="md-code-block">
        {lang && <span className="md-code-lang">{lang}</span>}
        <pre><code>{codeLines.join("\n")}</code></pre>
      </div>);
      i++; continue;
    }

    if (line.includes("|") && line.trim().startsWith("|")) {
      const tableRows: string[][] = [];
      while (i < lines.length && lines[i].includes("|") && lines[i].trim().startsWith("|")) {
        const cols = lines[i].split("|").map(c => c.trim()).filter((_, idx, arr) => idx > 0 && idx < arr.length - 1);
        if (!cols.every(c => /^[-: ]+$/.test(c))) tableRows.push(cols);
        i++;
      }
      if (tableRows.length > 0) {
        result.push(<div key={key++} className="md-table-wrap"><table className="md-table">
          <thead><tr>{tableRows[0].map((h, j) => <th key={j}>{inlineMarkdown(h)}</th>)}</tr></thead>
          <tbody>{tableRows.slice(1).map((row, ri) => <tr key={ri}>{row.map((cell, ci) => <td key={ci}>{inlineMarkdown(cell)}</td>)}</tr>)}</tbody>
        </table></div>);
      }
      continue;
    }

    const headMatch = line.match(/^(#{1,4})\s+(.+)/);
    if (headMatch) {
      const level = headMatch[1].length;
      const Tag = (level <= 2 ? "h3" : "h4") as keyof React.JSX.IntrinsicElements;
      result.push(<Tag key={key++} className={`md-h${level}`}>{inlineMarkdown(headMatch[2])}</Tag>);
      i++; continue;
    }

    if (/^[-*+]\s/.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^[-*+]\s/.test(lines[i])) { items.push(lines[i].replace(/^[-*+]\s/, "")); i++; }
      result.push(<ul key={key++} className="md-ul">{items.map((it, j) => <li key={j}>{inlineMarkdown(it)}</li>)}</ul>);
      continue;
    }

    if (/^\d+\.\s/.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^\d+\.\s/.test(lines[i])) { items.push(lines[i].replace(/^\d+\.\s/, "")); i++; }
      result.push(<ol key={key++} className="md-ol">{items.map((it, j) => <li key={j}>{inlineMarkdown(it)}</li>)}</ol>);
      continue;
    }

    if (/^---+$/.test(line.trim())) { result.push(<hr key={key++} className="md-hr" />); i++; continue; }

    const paraLines: string[] = [];
    while (i < lines.length && lines[i].trim() !== "" && !lines[i].trimStart().startsWith("```") && !lines[i].includes("|") && !/^(#{1,4})\s/.test(lines[i]) && !/^[-*+]\s/.test(lines[i]) && !/^\d+\.\s/.test(lines[i])) {
      paraLines.push(lines[i]); i++;
    }
    if (paraLines.length > 0) result.push(<p key={key++} className="md-p">{inlineMarkdown(paraLines.join("\n"))}</p>);
  }
  return result;
}
