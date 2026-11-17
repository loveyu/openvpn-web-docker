// 在首屏绘制前恢复主题，避免手动主题与系统主题不同时闪烁。
try {
  const theme = localStorage.getItem("vpnweb-theme");
  if (theme === "light" || theme === "dark") {
    document.documentElement.dataset.theme = theme;
  }
} catch {
  // 禁用本地存储时仍可跟随系统，并在当前页面切换主题。
}
