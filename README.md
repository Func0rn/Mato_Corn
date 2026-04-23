# Mato Corn

Mato Corn 是一个面向中文用户、长期命令行任务和 AI Agent 工作流的终端工作区。

```
Corn 工作区
 -> Corn / Desk 任务区
 	-> Cornflake / Terminal 终端
```

每个 Desk 都绑定到一个真实文件夹：`~/mato_corn/<Desk 名称>`。每个 Terminal 默认从当前 Desk 的基准目录启动。Terminal Preset 用来定义“新建终端后自动执行什么”。

## 快速安装

在本仓库根目录执行：

```
cd /path/to/mato
 ./install.sh
```

安装脚本会做这些事：

- 使用当前源码构建：`cargo install --path . --force`
- 把二进制安装到：`~/.local/bin/mato`
- 如果已有 Mato daemon 正在运行，会先停止旧 daemon，除非你设置 `KEEP_DAEMON=1`
- 创建 Desk 根目录：`~/mato_corn`
- 如果还没有主题配置，会默认写入 `corn` 主题
- 如果目标位置已有旧二进制，会自动备份

如果你想清理/备份旧版 Office 状态：

```
RESET_STATE=1 ./install.sh
```

如果你想安装到其他目录：

```
INSTALL_DIR="$HOME/bin" ./install.sh
```

如果安装后仍然启动旧版本，检查 PATH 优先级：

```
which -a mato
 mato --version
```



## 启动

```
mato
```

升级后如果旧 daemon 还在运行，先重启 Mato：

```
mato --kill
 mato
```



## 核心概念

### Desk

![desk分区功能.gif](https://raw.githubusercontent.com/Func0rn/Tuchuang/main/img/20260423201114651.gif)

Desk 是任务区。按 `n` 可以新建 Desk。

新建 Desk 时 Mato 会弹出输入框，让你输入 Desk 名称，并自动绑定目录：

```
~/mato_corn/<Desk 名称>
```

如果目录已经存在，Mato 会直接复用；如果不存在，会自动创建。

Desk 名称输入框支持搜索已有目录：

- 普通文本：按 contains 模糊匹配
- 合法正则：按大小写不敏感正则匹配
- 非法正则：自动回退成普通文本匹配，不会卡住
- `Tab`：补全当前选中的目录名

### Alarm

![222.gif](https://raw.githubusercontent.com/Func0rn/Tuchuang/main/img/20260423201828746.gif)
 管理100个codex的你也想知道谁干完活了吧！干完了吱一声继续抽你

### Terminal Presets

![111.gif](https://raw.githubusercontent.com/Func0rn/Tuchuang/main/img/20260423201220125.gif)

Terminal Preset 是“终端默认动作模板”。它不是只执行一条命令，而是支持多行脚本。

打开 Preset 面板： F2

或者在顶部 Terminal 栏按： p

Preset 的行为：

- 选中某个 preset 后按 `Enter`，会把它设为当前默认 preset
- 之后每次新建 Cornflake，都会自动执行这个 preset
- 这个默认 preset 会一直生效，直到你切换到另一个 preset
- 默认 preset 会保存到 `~/.config/mato/state.json`

Preset 管理快捷键：

| KEY       | 作用                                          |
| --------- | --------------------------------------------- |
| `m`       | 进入/退出管理模式                             |
| `a`       | 新增 preset                                   |
| `e` / `r` | 编辑 preset                                   |
| `d` / `x` | 删除 preset，仅管理模式                       |
| `Ctrl+S`  | 编辑时保存                                    |
| `Enter`   | Name 字段进入 Script；Script 字段新增一行命令 |
| `Esc`     | 退出/取消                                     |

Preset 配置文件：

```
~/.config/mato/terminal_presets.json
```

示例 preset：

```
name: codex-danger
 script:
 pwd
 codex --dangerously-bypass-approvals-and-sandbox
```

执行方式：新建 Cornflake 后，Mato 会启动 shell，然后把 script 里的每个非空行依次发送给 shell。

## 鼠标、滚动和复制

Mato 默认捕获鼠标事件，因为点击 Desk、Terminal 和内容区需要可靠切换 focus。

常用交互：

- 普通 shell 下，内容区鼠标滚轮会滚动 Mato scrollback
- 如果内部程序启用了鼠标模式，例如 vim、less、某些 TUI，鼠标事件会透传给内部程序
- `Shift+PageUp` / `Shift+PageDown`：快速滚动 scrollback
- `F3`：进入全屏选择模式，只显示内部终端内容，并释放鼠标捕获
- 全屏选择模式下可以用宿主终端直接拖选复制内部终端文本
- 再按 `F3` 或 `Esc` 返回 Mato 鼠标模式

## 快捷键

| KEY                     | 作用                   | 场景                         |
| ----------------------- | ---------------------- | ---------------------------- |
| `Esc Esc`               | 进入 Jump Mode         | Content                      |
| `Esc`                   | 进入 Jump Mode         | Sidebar / Topbar             |
| `n`                     | 新建 Desk              | Sidebar                      |
| `n`                     | 新建 Cornflake         | Topbar                       |
| `p`                     | 打开 Terminal Presets  | Topbar                       |
| `F2`                    | 打开 Terminal Presets  | 全局                         |
| `F3`                    | 全屏选择复制模式       | 全局                         |
| `r`                     | 重命名 Desk / Terminal | Sidebar / Topbar             |
| `x`                     | 关闭 Desk / Terminal   | Sidebar / Topbar             |
| `s`                     | 主题设置               | Sidebar                      |
| `q`                     | 退出                   | Sidebar / Topbar / Jump Mode |
| `Shift+PageUp/PageDown` | 快速滚动 scrollback    | Content                      |

## 主题

默认主题是 `corn`：玉米须黄为主、淡绿色为辅、玉米须棕色文字。

主题配置文件：

```
~/.config/mato/theme.toml
```

显式设置为 corn：

name = "corn"

## 运行文件位置



```
~/.config/mato/state.json        # Desk/Terminal 状态和当前默认 preset
 ~/.config/mato/terminal_presets.json  # Terminal Presets
 ~/.config/mato/theme.toml        # 主题配置
 ~/.local/state/mato/daemon.sock     # daemon socket
 ~/.local/state/mato/daemon.log     # daemon 日志
 ~/mato_corn/              # Desk 绑定目录根路径


```



## 开发

检查构建：

```
cargo check
 cargo test
```

安装当前源码版本：

```
./install.sh
```

如果环境里没有 `rustfmt`，`cargo fmt` 可能不可用，但不影响 `cargo check` 和安装。
