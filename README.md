<p align="center"><strong>Codex CLI</strong> is a coding agent from OpenAI that runs locally on your computer.
<p align="center">
  <img src="https://github.com/openai/codex/blob/main/.github/codex-cli-splash.png" alt="Codex CLI splash" width="80%" />
</p>
</br>
If you want Codex in your code editor (VS Code, Cursor, Windsurf), <a href="https://developers.openai.com/codex/ide">install in your IDE.</a>
</br>If you want the desktop app experience, run <code>codex app</code> or visit <a href="https://chatgpt.com/codex?app-landing-page=true">the Codex App page</a>.
</br>If you are looking for the <em>cloud-based agent</em> from OpenAI, <strong>Codex Web</strong>, go to <a href="https://chatgpt.com/codex">chatgpt.com/codex</a>.</p>

---

## Quickstart

### Installing and running Codex CLI

Run the following on Mac or Linux to install Codex CLI:

```shell
curl -fsSL https://chatgpt.com/codex/install.sh | sh
```

Run the following on Windows to install Codex CLI:

```
powershell -ExecutionPolicy ByPass -c "irm https://chatgpt.com/codex/install.ps1 | iex"
```

Codex CLI can also be installed via the following package managers:

```shell
# Install using npm
npm install -g @openai/codex
```

```shell
# Install using Homebrew
brew install --cask codex
```

Then simply run `codex` to get started.

<details>
<summary>You can also go to the <a href="https://github.com/openai/codex/releases/latest">latest GitHub Release</a> and download the appropriate binary for your platform.</summary>

Each GitHub Release contains many executables, but in practice, you likely want one of these:

- macOS
  - Apple Silicon/arm64: `codex-aarch64-apple-darwin.tar.gz`
  - x86_64 (older Mac hardware): `codex-x86_64-apple-darwin.tar.gz`
- Linux
  - x86_64: `codex-x86_64-unknown-linux-musl.tar.gz`
  - arm64: `codex-aarch64-unknown-linux-musl.tar.gz`

Each archive contains a single entry with the platform baked into the name (e.g., `codex-x86_64-unknown-linux-musl`), so you likely want to rename it to `codex` after extracting it.

</details>

### 本分支特性：工具输出前置预压缩

这个分支是 `bilisheep/tool-output-pre-compression` 的策略分支，基于公共的 prompt/tool
观测能力继续推进“工具输出回灌前预压缩”。目标是在大型工具输出进入主模型上下文之前，先由确定性抽取
和小模型生成 evidence packet，从源头减少当前轮工具输出回灌 token。

当前分支已从公共基础分支中独立出预压缩配置骨架，配置项如下：

```toml
[tool_output_compression]
enabled = false
model = "gpt-5.4-mini"
apply_to = ["exec_command"]
threshold_tokens = 1200
target_tokens = 800
raw_store_max_bytes_per_output = 1048576
timeout_ms = 8000
```

参数含义：

- `enabled`：是否开启工具输出前置预压缩，默认关闭。
- `model`：用于生成 evidence packet 的小模型，默认 `gpt-5.4-mini`。
- `apply_to`：允许预压缩的工具名，默认只面向 `exec_command`。
- `threshold_tokens`：触发预压缩的大输出 token 门槛。
- `target_tokens`：压缩后 evidence packet 的目标 token 大小。
- `raw_store_max_bytes_per_output`：单条原始工具输出本地保留上限。
- `timeout_ms`：小模型压缩链路超时时间。

预期完整链路是：工具原始输出先进入本地 raw store；主模型默认只收到包含命令、退出码、路径行号、
错误栈、测试失败、diff hunk、hash 和 `output_ref` 的 evidence packet；必要时再通过受限展开工具读取
原始片段。这个分支不包含后置 `trim_prompt_context` 剪枝，那部分属于
`bilisheep/tool-output-relevance-pruning`。

### Using Codex with your ChatGPT plan

Run `codex` and select **Sign in with ChatGPT**. We recommend signing into your ChatGPT account to use Codex as part of your Plus, Pro, Business, Edu, or Enterprise plan. [Learn more about what's included in your ChatGPT plan](https://help.openai.com/en/articles/11369540-codex-in-chatgpt).

You can also use Codex with an API key, but this requires [additional setup](https://developers.openai.com/codex/auth#sign-in-with-an-api-key).

## Docs

- [**Codex Documentation**](https://developers.openai.com/codex)
- [**Contributing**](./docs/contributing.md)
- [**Installing & building**](./docs/install.md)
- [**Open source fund**](./docs/open-source-fund.md)

This repository is licensed under the [Apache-2.0 License](LICENSE).
