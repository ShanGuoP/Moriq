# 人脸识别 API

> 按 2026-11-07 当前源码维护。默认模型为 YuNet + SFace CPU；Qwen 人脸接口是兼容或诊断路径。

## 应用层接口

人物变更通过 [Service](../../../../crates/moriq-app/src/catalog.rs) 的写入队列执行。进入项目时使用 `service.for_project(Some(project))` 冻结范围，调用时传入活动 `LibraryId`。

| 接口 | 行为 |
|---|---|
| `queue_media_job(library, JobKind::Analysis(ModelId::Faces))` | 登记当前范围的 CPU 人脸任务，冻结可处理的当前展示版本 |
| `rename_person(library, person, name)` | 更新人物名称，保留照片及分组身份 |
| `delete_person(library, person)` | 删除人物分组，保留原片和人工未归组检测 |
| `merge_people(library, source, target)` | 合并人物关联并刷新代表向量 |
| `faces(library, asset)` | 读取素材当前展示版本的人脸及人物信息 |
| `assign_face(library, face, person)` | 保存人工人脸归属 |
| `person_portrait(library, person)` | 从有效当前版本读取、裁剪代表头像 |

`rename_person`、`delete_person`、`merge_people` 校验人物所属项目；过期资源库请求不能变更当前库。任务控制和失败重试沿用 [任务管理 API](../任务管理%20API.md)。

## 库层人物接口

以下方法定义于 [analysis.rs](../../../../crates/moriq-library/src/ai/analysis.rs)，在单写者上下文中调用：

```rust
pub fn people(&self) -> Result<Vec<Person>>;
pub fn rename_person(&mut self, id: &str, name: &str) -> Result<()>;
pub fn delete_person(&mut self, id: &str) -> Result<()>;
pub fn merge_people(&mut self, source: &str, target: &str) -> Result<()>;
pub fn faces(&self, asset: AssetId) -> Result<Vec<Face>>;
pub fn assign_face(&mut self, face: &str, person: Option<&str>) -> Result<()>;
```

名称去除首尾空白后不能为空，UTF-8 长度不超过 128 字节。删除分组在事务中把已有检测更新为 `person_id=NULL, manual=1`，再删除人物；代表向量和工作区归属随人物记录清理。原照片、文件、标签及版本保留，重新分析已有检测时尊重人工取消归组。

`assign_face` 的 `person=None` 表示创建一个新的未命名人物用于这张脸，不是删除人物或把照片移入回收站。

## 默认 CPU 运行器

[cpu_faces.rs](../../../../crates/moriq-ai/src/cpu_faces.rs) 提供：

```rust
pub fn available() -> bool;
pub fn ready() -> Result<PathBuf>;
pub fn infer(image: &Path, cancel: &AtomicBool) -> Result<serde_json::Value>;
pub fn unload() -> Result<()>;
```

- `available` 检查必需文件是否存在；`ready` 按构建内嵌摘要校验运行目录，启动时执行完整校验。
- `infer` 接收有界预览 PNG，返回 `faces` 数组；每项包含归一化 `[x, y, width, height]` 框和 128 维向量。成功结果可为空数组。
- 运行器复用受控子进程，支持取消、超时及回执预算限制；忙碌时显式卸载返回错误。
- 全局 `set_keep_loaded` 和 `batch_lease` 管理按需驻留及批次复用，调用方不直接管理 OpenCV 进程。

## 结果状态与查询

`Query.face_analyzed: Option<bool>` 以当前展示版本的成功 `ai_results` 为依据，包含模型 `faces` 及兼容的 `qwen35-4b/faces`。`Some(true)` 匹配已完成，`Some(false)` 匹配未完成，`None` 不筛选。

“已完成人脸分析”卡片包含成功返回空数组的素材，不保证存在人物分组；“待整理”排除这些已完成结果。该状态不写入 Qwen 媒介或主题，不等于内容分类完成。

## 当前 UI 约束

[people_actions.rs](../../../../crates/moriq-ui/src/people_actions.rs) 为内外卡片提供独立操作，编辑回执绑定库、项目和请求编号。取消不提交，保存时拒绝重复提交，失败保留表单输入。后台头像及图库更新说明见 [人脸识别流水线](../../../架构设计/数据流设计/AI%20推理数据流/人脸识别流水线.md)。

## 兼容入口

`moriq_ai::llama_faces` 与 `JobKind::QwenFaces` 仍供旧任务或诊断使用，不是默认 CPU 接口。当前 Qwen 模型卡片只创建内容分类和标签描述任务。使用新入口的操作说明见 [AI 工作室与人物整理](../../../AI%20工作室与人物整理.md)。
