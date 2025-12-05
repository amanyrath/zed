use crate::tests::{TestServer, room_participants};
use call::{ActiveCall, room, participant::AgentActivityStatus};
use gpui::{BackgroundExecutor, TestAppContext};
use rpc::proto;
use std::sync::{Arc, Mutex};

#[ctor::ctor]
fn init_logger() {
    zlog::init_test();
}

#[gpui::test]
async fn test_agent_activity_broadcast(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .make_contacts(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;

    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);

    // User A creates a room and invites User B
    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_b.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();

    // User B accepts the call
    executor.run_until_parked();
    active_call_b
        .update(cx_b, |call, cx| call.accept_incoming(cx))
        .await
        .unwrap();
    executor.run_until_parked();

    // Get room handles
    let room_a = active_call_a.read_with(cx_a, |call, _| call.room().unwrap().clone());
    let room_b = active_call_b.read_with(cx_b, |call, _| call.room().unwrap().clone());

    // Verify both users are in the room
    let participants_a = room_participants(&room_a, cx_a);
    assert!(
        participants_a.remote.contains(&"user_b".to_string()),
        "user_b should be in user_a's remote participants"
    );

    // User A sends an agent activity update
    let room_id = room_a.read_with(cx_a, |room, _| room.id());
    let user_a_id = client_a.user_id().unwrap();

    client_a.client().send(proto::UpdateAgentActivity {
        room_id,
        activity: Some(proto::AgentActivity {
            user_id: user_a_id,
            agent_type: "Anthropic".to_string(),
            status: proto::AgentActivityStatus::AgentActive as i32,
            prompt_summary: Some("Analyzing code".to_string()),
        }),
    }).unwrap();

    // Wait for the message to propagate
    executor.run_until_parked();

    // Verify User B receives the agent activity update
    room_b.read_with(cx_b, |room, _| {
        // Find User A in the remote participants
        let participant_a = room
            .remote_participants()
            .values()
            .find(|p| p.user.github_login == "user_a")
            .expect("user_a should be in remote participants");

        let activity = participant_a
            .agent_activity
            .as_ref()
            .expect("user_a should have agent activity");

        assert_eq!(activity.agent_type.as_ref(), "Anthropic");
        assert_eq!(activity.status, AgentActivityStatus::Active);
        assert_eq!(
            activity.prompt_summary.as_ref().map(|s| s.as_ref()),
            Some("Analyzing code")
        );
    });

    // User A sets agent to idle
    client_a.client().send(proto::UpdateAgentActivity {
        room_id,
        activity: Some(proto::AgentActivity {
            user_id: user_a_id,
            agent_type: "Anthropic".to_string(),
            status: proto::AgentActivityStatus::AgentIdle as i32,
            prompt_summary: None,
        }),
    }).unwrap();

    executor.run_until_parked();

    // Verify User B sees the idle status
    room_b.read_with(cx_b, |room, _| {
        let participant_a = room
            .remote_participants()
            .values()
            .find(|p| p.user.github_login == "user_a")
            .expect("user_a should be in remote participants");

        let activity = participant_a
            .agent_activity
            .as_ref()
            .expect("user_a should have agent activity");

        assert_eq!(activity.status, AgentActivityStatus::Idle);
    });
}

#[gpui::test]
async fn test_agent_doc_changed_notification(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .make_contacts(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;

    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);

    // User A creates a room and invites User B
    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_b.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();

    // User B accepts the call
    executor.run_until_parked();
    active_call_b
        .update(cx_b, |call, cx| call.accept_incoming(cx))
        .await
        .unwrap();
    executor.run_until_parked();

    // Get room handles
    let room_a = active_call_a.read_with(cx_a, |call, _| call.room().unwrap().clone());
    let room_b = active_call_b.read_with(cx_b, |call, _| call.room().unwrap().clone());

    // Subscribe to AgentDocChanged events on room_b
    let doc_changed_events: Arc<Mutex<Vec<(u64, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let events_clone = doc_changed_events.clone();
    cx_b.update(|cx| {
        cx.subscribe(&room_b, move |_, event: &room::Event, _| {
            if let room::Event::AgentDocChanged { user_id, path } = event {
                events_clone.lock().unwrap().push((*user_id, path.clone()));
            }
        })
        .detach();
    });

    // User A sends an agent doc changed notification
    let room_id = room_a.read_with(cx_a, |room, _| room.id());
    let user_a_id = client_a.user_id().unwrap();

    client_a.client().send(proto::AgentDocChanged {
        room_id,
        user_id: user_a_id,
        path: ".agent-docs/analysis.md".to_string(),
    }).unwrap();

    // Wait for the message to propagate
    executor.run_until_parked();

    // Verify User B received the notification
    let events = doc_changed_events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, user_a_id);
    assert_eq!(events[0].1, ".agent-docs/analysis.md");
}

#[gpui::test]
async fn test_agent_activity_cleared_on_leave(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .make_contacts(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;

    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);

    // User A creates a room and invites User B
    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_b.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();

    // User B accepts the call
    executor.run_until_parked();
    active_call_b
        .update(cx_b, |call, cx| call.accept_incoming(cx))
        .await
        .unwrap();
    executor.run_until_parked();

    // Get room handles
    let room_a = active_call_a.read_with(cx_a, |call, _| call.room().unwrap().clone());
    let room_b = active_call_b.read_with(cx_b, |call, _| call.room().unwrap().clone());

    // User A sends an agent activity update
    let room_id = room_a.read_with(cx_a, |room, _| room.id());
    let user_a_id = client_a.user_id().unwrap();

    client_a.client().send(proto::UpdateAgentActivity {
        room_id,
        activity: Some(proto::AgentActivity {
            user_id: user_a_id,
            agent_type: "Anthropic".to_string(),
            status: proto::AgentActivityStatus::AgentActive as i32,
            prompt_summary: Some("Working on task".to_string()),
        }),
    }).unwrap();

    executor.run_until_parked();

    // Verify User B sees the activity
    room_b.read_with(cx_b, |room, _| {
        let participant_a = room
            .remote_participants()
            .values()
            .find(|p| p.user.github_login == "user_a");
        assert!(participant_a.is_some());
        assert!(participant_a.unwrap().agent_activity.is_some());
    });

    // User A leaves the room
    active_call_a
        .update(cx_a, |call, cx| call.hang_up(cx))
        .await
        .unwrap();
    executor.run_until_parked();

    // Verify User B no longer sees User A in participants
    room_b.read_with(cx_b, |room, _| {
        let participant_a = room
            .remote_participants()
            .values()
            .find(|p| p.user.github_login == "user_a");
        assert!(
            participant_a.is_none(),
            "user_a should no longer be in remote participants after leaving"
        );
    });
}
