import { memo } from "react";
import { PrismLight as SyntaxHighlighter } from "react-syntax-highlighter";
import logLanguage from "refractor/log";
import stripAnsi from "strip-ansi";

SyntaxHighlighter.registerLanguage("log", logLanguage);

export const LogOutput = memo(function LogOutput({ content }: { content: string }) {
  return (
    <SyntaxHighlighter
      language="log"
      className="logs log-highlight"
      useInlineStyles={false}
      wrapLongLines
    >
      {stripAnsi(content) || "暂无日志。启动服务后在这里查看输出。"}
    </SyntaxHighlighter>
  );
});
