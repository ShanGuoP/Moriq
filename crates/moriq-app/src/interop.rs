//! Durable Synco handoff.
use super::*;

impl Service {
    /// 为失败的 Synco 任务创建重试并协调重新发送。
    pub async fn retry_synco(&self, id: LibraryId, task: String) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .retry_synco(&task)
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .await
    }
    /// 冻结素材任务后连接 Synco 发送请求，保存发送失败供后续重试。
    pub async fn prepare_synco(
        &self,
        id: LibraryId,
        assets: Vec<AssetId>,
        project: Option<ProjectId>,
    ) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .prepare_synco(&assets, project)
                .map_err(|e| e.to_string())
        })
        .await
    }
    /// 轮询待处理任务，校验并登记返回版本，再发送幂等确认。
    pub async fn synco_poll(&self, id: LibraryId) -> Result<usize> {
        self.call(move |active| {
            check(active, id)?
                .cleanup_synco_copies()
                .map_err(|e| e.to_string())
        })
        .await?;
        let tasks = self
            .call(move |active| {
                check(active, id)?
                    .synco_pending()
                    .map_err(|e| e.to_string())
            })
            .await?;
        if tasks.is_empty() {
            return Ok(0);
        }
        let settings = self.settings().await?;
        let client = if settings.synco.auto_start {
            moriq_synco::Client::discover_or_start(settings.synco.executable).await?
        } else {
            moriq_synco::Client::discover().await?
        };
        let mut finished = 0;
        let mut failure = None;
        for task in tasks {
            let task_id = task.id.clone();
            let task_client = client.clone();
            let service = self.clone();
            let result = async {
                let request: serde_json::Value =
                    serde_json::from_str(&task.request).map_err(|e| e.to_string())?;
                let client_id = request["client_id"]
                    .as_str()
                    .ok_or("交接请求缺少客户端身份")?;
                let ack = if let Some(ack) = task.ack {
                    Some(
                        serde_json::from_str::<serde_json::Value>(&ack)
                            .map_err(|e| e.to_string())?,
                    )
                } else {
                    let response = match task_client.get(&task_id, client_id).await? {
                        Some(value) => value,
                        None if task.state == "queued" => {
                            task_client.send(&request).await?;
                            task_client
                                .get(&task_id, client_id)
                                .await?
                                .ok_or("Synco 接受后尚未提供任务状态，请重试")?
                        }
                        None => {
                            return Err(
                                "Synco 中找不到已送达的任务，请检查是否连接到原来的实例".into()
                            );
                        }
                    };
                    let task = task_id.clone();
                    service
                        .call(move |active| {
                            check(active, id)?
                                .receive_synco(&task, &response)
                                .map_err(|e| e.to_string())
                        })
                        .await?
                };
                if let Some(ack) = ack {
                    task_client.ack(&task_id, &ack).await?;
                    let task = task_id.clone();
                    service
                        .call(move |active| {
                            check(active, id)?
                                .synco_acked(&task)
                                .map_err(|e| e.to_string())
                        })
                        .await?;
                    finished += 1;
                }
                Ok::<(), String>(())
            }
            .await;
            if let Err(error) = result {
                let task = task.id;
                let detail = error.clone();
                let retained = self
                    .call(move |active| {
                        check(active, id)?
                            .synco_error(&task, &detail)
                            .map_err(|e| e.to_string())
                    })
                    .await?;
                if !retained {
                    continue;
                }
                failure = Some(error);
            }
        }
        if let Some(error) = failure {
            return Err(error);
        }
        Ok(finished)
    }
}
