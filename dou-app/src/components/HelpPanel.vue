<script setup lang="ts">
// R2 帮助页：新用户第一站——三个核心概念 + 常见问题 + 文档入口占位。
// 文档站未上线前，入口按钮给出明确占位说明，不误导。
import { openLocalTab } from '../core/useTerminal'
import { showConnectForm } from '../core/useTerminal'
import { navigateTo } from '../core/useNav'

function gotoConnect() {
  navigateTo('term')
  showConnectForm.value = true
}
</script>

<template>
  <div class="wizard">
    <h3 class="sec-title">三个核心概念</h3>
    <ul class="help-list">
      <li><b>连接网关</b>：本机后台服务（127.0.0.1），管理凭据与安全策略，不接触外部网络。</li>
      <li><b>远程主机</b>：你通过 SSH 连接的服务器，终端 / 文件 / 传输都围绕它工作。</li>
      <li><b>本地 Shell</b>：不连接任何主机、只在你电脑上运行的终端，随时可用。</li>
    </ul>
  </div>

  <div class="wizard">
    <h3 class="sec-title">最快上手路径</h3>
    <ol class="help-list">
      <li>点侧栏「终端」，在新建连接卡片里填主机地址、端口、用户名，选认证方式后连接。</li>
      <li>连上后左侧「文件」面板自动列出远端目录，点文件即可编辑保存。</li>
      <li>常用主机会存进「连接管理」，下次双击档案即可一键连接。</li>
    </ol>
    <div class="row">
      <button type="button" class="btn ok" @click="gotoConnect">立即新建连接</button>
      <button type="button" class="btn" @click="openLocalTab()">先打开本地 Shell</button>
    </div>
  </div>

  <div class="wizard">
    <h3 class="sec-title">终端快捷键</h3>
    <ul class="help-list keys">
      <li><kbd>Ctrl+Shift+C</kbd> 复制选区；<kbd>Ctrl+Shift+V</kbd> 粘贴（<kbd>Ctrl+C</kbd> 有选区时也复制，无选区时放行给终端中断）。</li>
      <li><kbd>Ctrl+Tab</kbd> 下一标签，<kbd>Ctrl+Shift+Tab</kbd> 上一标签（环形切换）。</li>
      <li><kbd>Ctrl+Shift+W</kbd> 关闭当前标签；关闭刻意避开 <kbd>Ctrl+W</kbd>，把终端删词快捷键留给 shell。</li>
      <li><kbd>Ctrl+F</kbd> 打开查找（自动聚焦全选）；<kbd>Enter</kbd> 下一个、<kbd>Shift+Enter</kbd> 上一个、<kbd>Esc</kbd> 关闭并清高亮；默认忽略大小写、非正则，每个会话独立检索。</li>
    </ul>
  </div>

  <div class="wizard">
    <h3 class="sec-title">文件与传输</h3>
    <ul class="help-list">
      <li>左侧「文件」面板点目录下钻、「↑ 上级」返回；顶部可过滤文件名、显示/隐藏文件；点文件在编辑器打开保存。</li>
      <li><b>上传：</b>把本地文件直接拖进文件面板，即按当前远端目录入队上传；暂不支持拖拽目录，目录同步请走「传输队列」。</li>
      <li>下载与上传进度都在「📤 传输队列」查看，支持断点续传；大文件可暂停 / 继续 / 取消。</li>
    </ul>
  </div>

  <div class="wizard">
    <h3 class="sec-title">安全与隐私</h3>
    <ul class="help-list">
      <li><b>主机密钥（TOFU）：</b>首次连接记录指纹，提示你肉眼核对；指纹与记录不符会硬性拒绝连接（防中间人），不会静默放行。确认对方合法换钥后，可到「主机密钥」页遗忘旧记录再重连。</li>
      <li><b>私钥只在内存解密</b>，用完即清，不落盘；<b>密码不落盘、不写日志、不重放</b>，连接那一瞬间才经内存传给 SSH。</li>
      <li>Agent 执行的命令经三档策略分段把关，高危动作进审批队列；操作记录在「审计中心」可回溯。</li>
      <li><b>换机迁移：</b>「连接管理」页可导出加密档案备份（密文，不解密）；新机器导入后用<b>原主密码</b>解锁即可，无需重新录入私钥。导入前会自动备份现有保险库。</li>
    </ul>
  </div>

  <div class="wizard">
    <h3 class="sec-title">常见问题</h3>
    <ul class="help-list">
      <li><b>连不上主机？</b>看终端区的红色提示：地址/端口写错、主机不在线、密钥或密码不对是三大主因；密钥连不上可改密码认证。</li>
      <li><b>「网关离线」？</b>说明本机后台服务没起来，重启应用即可恢复；HTTP 管理功能会暂时不可用，终端不受影响。</li>
      <li><b>密码会被保存吗？</b>不会。密码只在连接那一瞬间经内存传给 SSH，不落盘、不写日志、不重放。</li>
      <li><b>忘记主密码怎么办？</b>应用无后门、无找回渠道；忘记主密码 = 加密档案不可恢复，初始化前请务必牢记。</li>
      <li><b>高级功能在哪？</b>侧栏底部「高级」分组里：审批、策略、审计、凭据与主机密钥等，日常用不到可以一直收着。</li>
    </ul>
  </div>

  <div class="wizard">
    <h3 class="sec-title">文档</h3>
    <div class="hint">完整文档站筹备中（占位入口）。当前版本的使用说明以本页为准；安装、部署与安全设计详见随包附带的 <code class="mono">docs/</code> 目录。</div>
  </div>
</template>

<style scoped>
.sec-title { margin: 0 0 10px; font-size: var(--dou-fs-body); color: var(--dou-accent-strong); }
.sec-title::before { content: "// "; color: var(--dou-dim); font-weight: 400; }
.help-list { margin: 0; padding-left: 20px; line-height: 1.9; font-size: var(--dou-fs-sm); color: var(--dou-text); max-width: 68ch; }
.help-list b { color: var(--dou-accent-strong); font-weight: 600; }
.help-list kbd {
  display: inline-block;
  padding: 0 5px;
  border: 1px solid var(--dou-border);
  border-radius: 4px;
  background: var(--dou-panel);
  font-family: inherit;
  font-size: 11px;
  color: var(--dou-text);
}
.help-list.keys li { margin-bottom: 6px; }
</style>
