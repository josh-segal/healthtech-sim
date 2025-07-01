/// Log a claim processing event with standardized format
/// 
/// Used by all components to track claim lifecycle events
/// Format: [component][claim:claim_id][event] message
pub fn log_claim_event(component: &str, claim_id: &str, event: &str, message: &str) {
    println!(
        "[{}][claim:{}][{}] {}\n",
        component,
        claim_id,
        event,
        message
    );
} 