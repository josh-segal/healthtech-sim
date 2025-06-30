
pub fn log_claim_event(component: &str, claim_id: &str, event: &str, message: &str) {
    println!(
        "[{}][claim:{}][{}] {}\n",
        component,
        claim_id,
        event,
        message
    );
} 