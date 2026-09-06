# jabkit-rs 工具级待修清单

> 本文件只记录**工具级**问题（CLI / 退出码 / BibTeX 生成 / keyring / release / README）。
> Provider 级问题（API 调用、serde、字段映射、限流）→ `~/.hermes/profiles/science/skills/jabkit-provider-diagnostics/SKILL.md`。
>
> 来源：2026-09-06 外部评审 + 本地源码复核 + 动态复现。

## 已修（2026-09-06）

### arXiv eprint 字段被覆盖为字面量 "arXiv"
- **位置**: `src/provider/arxiv.rs` `</entry>` handler
- **现象**: `eprint = {arXiv}` 而非 `eprint = {2306.12345}`
- **修复**: 删除 `</entry>` 里的 `set_field(Field::Eprint, "arXiv")` 覆盖
- **回归测试**: `main.rs` `test_arxiv_eprint_not_overwritten`（Atom XML 样本，断言 eprint 以真实 ID 开头）
- **实测**: `eprint = {1101.4081v3}`, `eprint = {2306.12345v2}`, `eprint = {nlin/0412031v1}`

### doi-to-bibtex 退出码恒 0
- **位置**: `src/main.rs` `Commands::DoiToBibtex`
- **现象**: 全部 DOI 404 时 exit 0，stdout 空
- **修复**: 加 `failures` 计数，`failures > 0` 时 `anyhow::bail!` → exit 1
- **实测**: 2×404 → exit=1；1 good + 1 fake → exit=1, stdout 有 1 条 BibTeX

### README 全面漂移
- **现象**: README 是 5-provider 时代写的，命令/选项/数量全部过时
- **修复**: 重写 README，对齐 `src/cli.rs` 实际定义（26 providers、`--provider` 必填、`--limit`、`get-by-id --provider --id`）
- **验证**: 所有 Quick Start 命令实测通过（fetch/doi-to-bibtex/list-providers/get-by-id 均 OK）

### Crossref fetch_by_id 缺 pmid/abstract/subtitle（字段不对称）
- **位置**: `src/provider/crossref.rs` `fetch_by_id()`
- **现象**: 同一 DOI，`fetch` 路径有 subtitle+pmid+abstract，`get-by-id` 路径没有
- **修复**: 给 `fetch_by_id` 补上 subtitle、PMID、abstract 处理（与 search 路径对齐）
- **实测**: 代码对齐完成；实测 DOI 是否含 pmid/abstract 取决于上游数据（Crossref 非所有条目都有）

### `ApiKeys` 派生 `Debug` 含明文 key
- **位置**: `src/keyring.rs`
- **现象**: `println!("{:?}", api_keys)` 会打印所有 API key 明文
- **修复**: 自定义 `Debug` impl，所有 key 字段显示 `***`
- **实测**: 编译通过，key 不再明文输出

### 所有 provider 无 HTTP timeout
- **位置**: `src/provider/mod.rs` + 全部 provider 文件
- **现象**: `reqwest::Client::new()` 无 timeout，网络挂起时 CLI 永久阻塞
- **修复**: 新增 `provider::http_client()` 共享客户端（30s request timeout + 10s connect timeout），全部 17 个 provider 文件改用
- **实测**: 编译通过，13/13 测试通过

### provider 数量表述不一致
- **位置**: `src/cli.rs` after_help
- **现象**: after_help 写 "Providers (25)"，fetch doc 写 "26 providers"
- **修复**: after_help 改为 "Providers (26)"
- **实测**: `--help` 输出 "Providers (26)" ✓

### arXiv 条目无 source 标注
- **位置**: `src/provider/arxiv.rs`
- **现象**: arXiv 条目 `@article` 无 journal 字段，biblatex 无法识别为 preprint
- **修复**: `<id>` 解析时设置 `Note = "arXiv"`；`<arxiv:comment>` 时追加到 Note（`; ` 分隔）
- **实测**: `note = {arXiv; 10 pages, 3 figures, 11 references}` ✓

### arXiv 搜索参数只 `replace(' ', "+")` 不编码
- **位置**: `src/provider/arxiv.rs:18`
- **现象**: 含 `&`/`=` 等字符的查询可能出错
- **修复**: 新增 `urlencoding()` 函数（percent-encoding，空格→`+`），替换原 `replace`
- **实测**: 编译通过

### release.yml 配置问题
- **位置**: `.github/workflows/release.yml`
- **现象**: macOS 目标在 ubuntu-latest 上跑（无交叉工具链）、无测试步骤、产物名与 README 不一致
- **修复**:
  - 加 `test` job（`cargo test --release`），build 依赖 test
  - macOS 目标改用 `macos-latest` runner（原生编译）
  - 加 `aarch64-unknown-linux-gnu` 目标
  - `fail-fast: false`（一个目标失败不阻塞其他）
  - 产物重命名为 `jabkit-linux-x86_64`/`jabkit-linux-aarch64`/`jabkit-windows-x86_64.exe`/`jabkit-macos-x86_64`/`jabkit-macos-aarch64`
  - 与 README 下载路径对齐
- **实测**: YAML 语法正确（lint ok），需 push tag 触发实际验证

### BibTeX 引用键无冲突消解
- **位置**: `src/main.rs` Fetch 循环
- **现象**: 同作者同年多篇文章 → 相同引用键（如 `yang2026` 出现两次）
- **修复**: 批次内 `HashMap<String, usize>` 计数，重复键加字母后缀（`yang2026` → `yang2026a` → `yang2026b`）
- **回归测试**: `main.rs` `test_citation_key_dedup`（3 次同 key → 3 个唯一键）
- **实测**: 14/14 测试通过

### 输出流不一致（print vs println）
- **位置**: `src/main.rs` Fetch/GetById 循环
- **现象**: `print!` 无换行，管道拼接时条目粘连
- **修复**: 全部改为 `println!`（Fetch 循环 + GetById 单条）
- **实测**: 编译通过，输出格式正确

## 待修

### P2: 字段格式化无括号平衡校验
- **位置**: `src/bibtex.rs` `format_bibtex_value()`
- **现象**: 上游数据含不平衡 `{}` 时，生成的 BibTeX 可能无法被解析器正确解析
- **修复方向**: 输出前校验括号平衡，不平衡时转义或丢弃
- **优先级**: 低（上游数据质量差时才会触发）

### P2: arXiv 条目类型固定 `@article`
- **位置**: `src/provider/arxiv.rs`
- **现象**: arXiv preprint 应输出 `@misc` 或 `@article` + `eprinttype = {arxiv}`（biblatex 标准）
- **当前**: 输出 `@article` + `note = {arXiv}`，可用但非 biblatex 最优
- **修复方向**: 加 `eprinttype` 字段或改用 `@misc`
- **优先级**: 低（当前输出可用，仅 biblatex 排版非最优）
