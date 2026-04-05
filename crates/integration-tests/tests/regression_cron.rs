//! Cron 服务与 store 回归测试

use stormclaw_services::cron::{every_job, CronService};
use tempfile::tempdir;

#[tokio::test]
async fn cron_service_add_list_persist() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("cron").join("jobs.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();

    let job = every_job("t1".into(), "hello".into(), 60);
    let id_prefix = job.id.clone();

    let svc = CronService::new(path.clone()).await.unwrap();
    svc.start().await.unwrap();
    let added = svc.add_job(job).await.unwrap();
    assert_eq!(added.name, "t1");

    let list = svc.list_jobs(false).await;
    assert_eq!(list.len(), 1);
    assert!(list[0].id.starts_with(&id_prefix) || list[0].id == id_prefix);

    drop(svc);

    let svc2 = CronService::new(path.clone()).await.unwrap();
    svc2.start().await.unwrap();
    let list2 = svc2.list_jobs(false).await;
    assert_eq!(list2.len(), 1);
    assert_eq!(list2[0].payload.message, "hello");
}

#[tokio::test]
async fn cron_enable_job_updates_store() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("cron").join("jobs.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();

    let job = every_job("e1".into(), "m".into(), 120);
    let jid = job.id.clone();

    let svc = CronService::new(path.clone()).await.unwrap();
    svc.start().await.unwrap();
    svc.add_job(job).await.unwrap();

    let updated = svc.enable_job(&jid, false).await.unwrap();
    assert!(updated.is_some());
    assert!(!updated.unwrap().enabled);

    let list = svc.list_jobs(true).await;
    assert_eq!(list.len(), 1);
    assert!(!list[0].enabled);
}
