# Real-Time Architecture

## Overview

The NodeSpace Real-Time Architecture enables live data synchronization across all user interfaces through QueryNodes, event-driven updates, and intelligent change propagation. The system ensures that when data changes in one location, all dependent views update instantly without manual refresh, creating a collaborative and responsive user experience.

## Core Components

### QueryNode System

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryNode {
    pub base: TextNode,                                    // Inherits hierarchical capabilities
    pub query: QueryDefinition,                           // What data to show
    pub view_calculated_fields: Vec<ViewCalculatedField>, // Query-specific computations
    pub results: Vec<String>,                             // Cached result node IDs
    pub last_executed: DateTime<Utc>,
    pub auto_refresh: bool,                               // Should auto-update on changes
    pub refresh_triggers: Vec<RefreshTrigger>,            // What changes trigger updates
    pub subscription_id: String,                          // Unique subscription identifier
    pub performance_metrics: QueryPerformanceMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryDefinition {
    pub name: String,                                     // "Tasks Due This Week"
    pub description: String,                              // User-friendly description
    pub entity_types: Vec<String>,                        // ["task", "project"]
    pub filters: Vec<QueryFilter>,                        // Filtering conditions
    pub sort_by: Vec<SortCriteria>,                       // Ordering rules
    pub limit: Option<usize>,                             // Result limit
    pub offset: Option<usize>,                            // Pagination offset
    pub include_calculated_fields: Vec<String>,           // Which calculated fields to include
    pub search_text: Option<String>,                      // Full-text search query
    pub date_range: Option<DateRange>,                    // Time-based filtering
    pub aggregations: Vec<AggregationDefinition>,         // GROUP BY operations
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryFilter {
    pub field_name: String,                               // Field to filter on
    pub operator: FilterOperator,                         // How to compare
    pub value: QueryValue,                                // What to compare against
    pub case_sensitive: bool,                             // For string comparisons
    pub negate: bool,                                     // Invert the condition
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueryValue {
    Static(EntityValue),                                  // Fixed value: "VIP"
    Dynamic(String),                                      // Expression: "TODAY() - 30"
    Reference(String),                                    // Field reference: "manager.salary"
    Parameter(String),                                    // User parameter: "${department}"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RefreshTrigger {
    FieldChange { 
        entity_type: String, 
        field: String,
        condition: Option<String>,                        // Only trigger if condition met
    },
    NodeCreate { entity_type: String },
    NodeDelete { entity_type: String },
    TimeInterval { 
        minutes: u32,
        only_during_hours: Option<(u8, u8)>,             // Business hours optimization
    },
    Manual,
    External { 
        source: String,                                   // External system name
        event_type: String,                               // Type of external event
    },
}
```

### Query Result Manager

```rust
pub struct QueryResultManager {
    active_queries: HashMap<String, ActiveQuery>,
    change_listeners: HashMap<String, Vec<String>>,       // field -> query_ids
    update_coordinator: UpdateCoordinator,
    subscription_manager: SubscriptionManager,
    performance_monitor: QueryPerformanceMonitor,
    cache_manager: QueryCacheManager,
}

#[derive(Debug)]
struct ActiveQuery {
    query_node_id: String,
    query: QueryDefinition,
    current_results: Vec<String>,                         // Current result node IDs
    result_metadata: HashMap<String, ResultMetadata>,     // Metadata per result
    subscribers: Vec<QuerySubscriber>,                    // Who's watching this query
    last_update: DateTime<Utc>,
    execution_stats: QueryExecutionStats,
    dependency_hash: u64,                                 // Hash of dependent data
}

#[derive(Debug)]
struct QuerySubscriber {
    subscriber_id: String,                                // UI component or user session
    subscriber_type: SubscriberType,
    notification_preferences: NotificationPreferences,
    last_notified: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub enum SubscriberType {
    UIComponent { component_id: String },
    UserSession { user_id: String, session_id: String },
    ExternalSystem { system_name: String, endpoint: String },
    WebhookEndpoint { url: String, auth_token: Option<String> },
}

impl QueryResultManager {
    pub async fn register_query(&mut self, query_node: &QueryNode, services: &Services) -> Result<(), Error> {
        // 1. Execute initial query
        let initial_execution = self.execute_query_with_metrics(&query_node.query, services).await?;
        
        // 2. Register change listeners based on query dependencies
        self.register_change_listeners(&query_node.id, &query_node.query).await?;
        
        // 3. Store active query state
        let active_query = ActiveQuery {
            query_node_id: query_node.id.clone(),
            query: query_node.query.clone(),
            current_results: initial_execution.results,
            result_metadata: initial_execution.metadata,
            subscribers: Vec::new(),
            last_update: Utc::now(),
            execution_stats: initial_execution.stats,
            dependency_hash: initial_execution.dependency_hash,
        };
        
        self.active_queries.insert(query_node.id.clone(), active_query);
        
        // 4. Set up automatic refresh if enabled
        if query_node.auto_refresh {
            self.schedule_refresh_triggers(&query_node.id, &query_node.refresh_triggers).await?;
        }
        
        Ok(())
    }
    
    pub async fn handle_entity_change(
        &mut self,
        change_event: EntityChangeEvent,
        services: &Services
    ) -> Result<Vec<QueryUpdate>, Error> {
        let start_time = Instant::now();
        
        // 1. Find affected queries using change listeners
        let affected_query_ids = self.find_affected_queries(&change_event);
        
        if affected_query_ids.is_empty() {
            return Ok(Vec::new());
        }
        
        // 2. Process updates in parallel for independent queries
        let update_tasks: Vec<_> = affected_query_ids.into_iter()
            .map(|query_id| self.process_query_update(query_id, &change_event, services))
            .collect();
        
        let update_results = futures::future::join_all(update_tasks).await;
        
        // 3. Collect successful updates and log failures
        let mut successful_updates = Vec::new();
        for result in update_results {
            match result {
                Ok(Some(update)) => successful_updates.push(update),
                Ok(None) => {}, // No update needed
                Err(e) => {
                    log::error!("Query update failed: {}", e);
                    // Continue processing other queries
                }
            }
        }
        
        // 4. Record performance metrics
        self.performance_monitor.record_batch_update(
            successful_updates.len(),
            start_time.elapsed()
        );
        
        Ok(successful_updates)
    }
    
    async fn process_query_update(
        &mut self,
        query_id: String,
        change_event: &EntityChangeEvent,
        services: &Services
    ) -> Result<Option<QueryUpdate>, Error> {
        let active_query = self.active_queries.get_mut(&query_id)
            .ok_or("Query not found")?;
        
        // 1. Check if this change actually affects the query results
        if !self.change_affects_query(&change_event, &active_query.query) {
            return Ok(None);
        }
        
        // 2. Re-execute query
        let new_execution = self.execute_query_with_metrics(&active_query.query, services).await?;
        
        // 3. Calculate incremental changes
        let changes = self.calculate_result_changes(
            &active_query.current_results,
            &new_execution.results,
            &active_query.result_metadata,
            &new_execution.metadata
        );
        
        if changes.is_empty() {
            return Ok(None); // No actual changes despite trigger
        }
        
        // 4. Update active query state
        active_query.current_results = new_execution.results;
        active_query.result_metadata = new_execution.metadata;
        active_query.last_update = Utc::now();
        active_query.execution_stats = new_execution.stats;
        active_query.dependency_hash = new_execution.dependency_hash;
        
        // 5. Create update notification
        let query_update = QueryUpdate {
            query_id: query_id.clone(),
            changes,
            trigger: UpdateTrigger::EntityChange {
                entity_id: change_event.entity_id.clone(),
                changed_fields: change_event.changed_fields.clone(),
                change_type: change_event.change_type.clone(),
            },
            execution_time: new_execution.stats.execution_time,
            updated_at: Utc::now(),
        };
        
        Ok(Some(query_update))
    }
    
    fn calculate_result_changes(
        &self,
        old_results: &[String],
        new_results: &[String],
        old_metadata: &HashMap<String, ResultMetadata>,
        new_metadata: &HashMap<String, ResultMetadata>
    ) -> Vec<ResultChange> {
        let mut changes = Vec::new();
        
        let old_set: HashSet<_> = old_results.iter().collect();
        let new_set: HashSet<_> = new_results.iter().collect();
        
        // Find added nodes
        for (position, node_id) in new_results.iter().enumerate() {
            if !old_set.contains(node_id) {
                changes.push(ResultChange::Added { 
                    node_id: node_id.clone(),
                    position,
                    metadata: new_metadata.get(node_id).cloned(),
                });
            }
        }
        
        // Find removed nodes
        for node_id in old_results {
            if !new_set.contains(node_id) {
                changes.push(ResultChange::Removed { 
                    node_id: node_id.clone(),
                    old_metadata: old_metadata.get(node_id).cloned(),
                });
            }
        }
        
        // Find moved nodes
        for (new_pos, node_id) in new_results.iter().enumerate() {
            if let Some(old_pos) = old_results.iter().position(|x| x == node_id) {
                if new_pos != old_pos {
                    changes.push(ResultChange::Moved {
                        node_id: node_id.clone(),
                        from_position: old_pos,
                        to_position: new_pos,
                    });
                }
            }
        }
        
        // Find updated nodes (same position, different metadata)
        for (position, node_id) in new_results.iter().enumerate() {
            if old_results.get(position) == Some(node_id) {
                let old_meta = old_metadata.get(node_id);
                let new_meta = new_metadata.get(node_id);
                
                if old_meta != new_meta {
                    changes.push(ResultChange::Updated {
                        node_id: node_id.clone(),
                        position,
                        old_metadata: old_meta.cloned(),
                        new_metadata: new_meta.cloned(),
                        changed_fields: self.calculate_changed_fields(old_meta, new_meta),
                    });
                }
            }
        }
        
        changes
    }
    
    fn find_affected_queries(&self, change_event: &EntityChangeEvent) -> Vec<String> {
        let mut affected_queries = HashSet::new();
        
        // Check field-specific listeners
        for changed_field in &change_event.changed_fields {
            let listener_key = format!("{}:{}", change_event.entity_type, changed_field);
            if let Some(query_ids) = self.change_listeners.get(&listener_key) {
                affected_queries.extend(query_ids.iter().cloned());
            }
        }
        
        // Check entity-type listeners (for creates/deletes)
        let entity_listener_key = format!("{}:*", change_event.entity_type);
        if let Some(query_ids) = self.change_listeners.get(&entity_listener_key) {
            affected_queries.extend(query_ids.iter().cloned());
        }
        
        // Check wildcard listeners (rare but possible)
        if let Some(query_ids) = self.change_listeners.get("*:*") {
            affected_queries.extend(query_ids.iter().cloned());
        }
        
        affected_queries.into_iter().collect()
    }
}

#[derive(Debug, Clone)]
pub struct EntityChangeEvent {
    pub entity_id: String,
    pub entity_type: String,
    pub changed_fields: Vec<String>,
    pub change_type: ChangeType,
    pub old_values: HashMap<String, EntityValue>,
    pub new_values: HashMap<String, EntityValue>,
    pub user_id: String,
    pub timestamp: DateTime<Utc>,
    pub transaction_id: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ChangeType {
    Create,
    Update,
    Delete,
    Restore, // Undo delete
}

#[derive(Debug, Clone)]
pub struct QueryUpdate {
    pub query_id: String,
    pub changes: Vec<ResultChange>,
    pub trigger: UpdateTrigger,
    pub execution_time: Duration,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub enum ResultChange {
    Added { 
        node_id: String, 
        position: usize,
        metadata: Option<ResultMetadata>,
    },
    Removed { 
        node_id: String,
        old_metadata: Option<ResultMetadata>,
    },
    Moved { 
        node_id: String, 
        from_position: usize, 
        to_position: usize 
    },
    Updated { 
        node_id: String, 
        position: usize,
        old_metadata: Option<ResultMetadata>,
        new_metadata: Option<ResultMetadata>,
        changed_fields: Vec<String>,
    },
}

#[derive(Debug, Clone)]
pub enum UpdateTrigger {
    EntityChange { 
        entity_id: String, 
        changed_fields: Vec<String>,
        change_type: ChangeType,
    },
    TimeInterval,
    Manual { user_id: String },
    External { source: String, event_type: String },
}
```

### Event-Driven Update System

```rust
pub struct UpdateCoordinator {
    event_bus: EventBus,
    query_manager: Arc<Mutex<QueryResultManager>>,
    validation_engine: Arc<ValidationEngine>,
    calculation_engine: Arc<EntityCalculationEngine>,
    notification_service: NotificationService,
    audit_logger: AuditLogger,
}

impl UpdateCoordinator {
    pub async fn handle_entity_change(&mut self, change: EntityChange) -> Result<(), Error> {
        let transaction_id = uuid::Uuid::new_v4().to_string();
        
        // 1. Validate the change
        let validation_result = self.validation_engine
            .validate_entity_change(&change).await?;
            
        if !validation_result.is_valid {
            return Err(ValidationError::from(validation_result));
        }
        
        // 2. Apply calculated field updates
        let calculation_updates = self.calculation_engine
            .recalculate_dependent_fields(&change).await?;
        
        // 3. Persist changes to storage
        self.persist_entity_changes(&change, &calculation_updates).await?;
        
        // 4. Create change event
        let change_event = EntityChangeEvent {
            entity_id: change.entity_id.clone(),
            entity_type: change.entity_type.clone(),
            changed_fields: change.changed_fields(),
            change_type: change.change_type.clone(),
            old_values: change.old_values.clone(),
            new_values: change.new_values.clone(),
            user_id: change.user_id.clone(),
            timestamp: Utc::now(),
            transaction_id: Some(transaction_id.clone()),
        };
        
        // 5. Publish to event bus
        self.event_bus.publish(Event::EntityChanged(change_event.clone())).await?;
        
        // 6. Update affected queries
        let query_updates = {
            let mut query_manager = self.query_manager.lock().await;
            query_manager.handle_entity_change(change_event.clone(), &self.get_services()).await?
        };
        
        // 7. Notify subscribers
        for update in &query_updates {
            self.notify_query_subscribers(update).await?;
        }
        
        // 8. Log audit trail
        self.audit_logger.log_entity_change(&change_event, &query_updates).await?;
        
        Ok(())
    }
    
    async fn notify_query_subscribers(&self, update: &QueryUpdate) -> Result<(), Error> {
        let query_manager = self.query_manager.lock().await;
        let active_query = query_manager.active_queries.get(&update.query_id)
            .ok_or("Query not found")?;
        
        // Notify each subscriber
        let notification_tasks: Vec<_> = active_query.subscribers.iter()
            .map(|subscriber| self.notify_single_subscriber(subscriber, update))
            .collect();
        
        // Execute notifications in parallel
        let results = futures::future::join_all(notification_tasks).await;
        
        // Log any notification failures
        for (i, result) in results.iter().enumerate() {
            if let Err(e) = result {
                log::error!("Failed to notify subscriber {}: {}", 
                    active_query.subscribers[i].subscriber_id, e);
            }
        }
        
        Ok(())
    }
    
    async fn notify_single_subscriber(
        &self,
        subscriber: &QuerySubscriber,
        update: &QueryUpdate
    ) -> Result<(), Error> {
        match &subscriber.subscriber_type {
            SubscriberType::UIComponent { component_id } => {
                self.notification_service.notify_ui_component(
                    component_id,
                    &QueryUpdateNotification {
                        query_id: update.query_id.clone(),
                        changes: update.changes.clone(),
                        trigger: update.trigger.clone(),
                    }
                ).await
            },
            SubscriberType::UserSession { user_id, session_id } => {
                self.notification_service.notify_user_session(
                    user_id,
                    session_id,
                    &update
                ).await
            },
            SubscriberType::WebhookEndpoint { url, auth_token } => {
                self.notification_service.send_webhook(
                    url,
                    auth_token.as_ref(),
                    &WebhookPayload::from(update)
                ).await
            },
            SubscriberType::ExternalSystem { system_name, endpoint } => {
                self.notification_service.notify_external_system(
                    system_name,
                    endpoint,
                    &update
                ).await
            },
        }
    }
}

pub struct EventBus {
    channels: HashMap<String, Vec<EventHandler>>,
    message_queue: Arc<Mutex<VecDeque<Event>>>,
    worker_handles: Vec<tokio::task::JoinHandle<()>>,
}

#[derive(Debug, Clone)]
pub enum Event {
    EntityChanged(EntityChangeEvent),
    QueryExecuted { query_id: String, execution_time: Duration },
    ValidationFailed { entity_id: String, violations: Vec<ValidationViolation> },
    SystemShutdown,
}

impl EventBus {
    pub async fn publish(&self, event: Event) -> Result<(), Error> {
        let mut queue = self.message_queue.lock().await;
        queue.push_back(event);
        Ok(())
    }
    
    pub fn subscribe<F>(&mut self, event_type: &str, handler: F) 
    where 
        F: Fn(Event) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send>> + Send + Sync + 'static
    {
        let handlers = self.channels.entry(event_type.to_string()).or_insert_with(Vec::new);
        handlers.push(Box::new(handler));
    }
    
    async fn process_events(&self) {
        loop {
            let event = {
                let mut queue = self.message_queue.lock().await;
                queue.pop_front()
            };
            
            if let Some(event) = event {
                self.dispatch_event(event).await;
            } else {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
    }
    
    async fn dispatch_event(&self, event: Event) {
        let event_type = match &event {
            Event::EntityChanged(_) => "entity_changed",
            Event::QueryExecuted { .. } => "query_executed",
            Event::ValidationFailed { .. } => "validation_failed",
            Event::SystemShutdown => "system_shutdown",
        };
        
        if let Some(handlers) = self.channels.get(event_type) {
            let handler_tasks: Vec<_> = handlers.iter()
                .map(|handler| handler(event.clone()))
                .collect();
                
            futures::future::join_all(handler_tasks).await;
        }
    }
}
```

### Real-Time UI Integration

```rust
// Tauri commands for real-time updates
#[tauri::command]
pub async fn subscribe_to_query(
    query_id: String,
    subscriber_id: String,
    subscriber_type: SubscriberType,
    app_handle: tauri::AppHandle,
    services: tauri::State<'_, Services>
) -> Result<(), Error> {
    let subscriber = QuerySubscriber {
        subscriber_id: subscriber_id.clone(),
        subscriber_type,
        notification_preferences: NotificationPreferences::default(),
        last_notified: Utc::now(),
    };
    
    let mut query_manager = services.query_manager.lock().await;
    query_manager.add_subscriber(&query_id, subscriber).await?;
    
    // Set up Tauri event emission for this subscription
    let app_handle_clone = app_handle.clone();
    services.notification_service.register_ui_callback(
        subscriber_id,
        Box::new(move |update: QueryUpdateNotification| {
            let app_handle = app_handle_clone.clone();
            Box::pin(async move {
                app_handle.emit_to(&update.query_id, "query-update", &update)
                    .map_err(|e| Error::NotificationError(e.to_string()))
            })
        })
    ).await;
    
    Ok(())
}

#[tauri::command]
pub async fn unsubscribe_from_query(
    query_id: String,
    subscriber_id: String,
    services: tauri::State<'_, Services>
) -> Result<(), Error> {
    let mut query_manager = services.query_manager.lock().await;
    query_manager.remove_subscriber(&query_id, &subscriber_id).await?;
    
    services.notification_service.unregister_ui_callback(&subscriber_id).await;
    
    Ok(())
}

#[tauri::command]
pub async fn execute_query_manually(
    query_id: String,
    user_id: String,
    services: tauri::State<'_, Services>
) -> Result<QueryExecutionResult, Error> {
    let mut query_manager = services.query_manager.lock().await;
    
    // Force re-execution regardless of cache
    let result = query_manager.force_execute_query(&query_id, &services).await?;
    
    // Emit update to all subscribers
    let update = QueryUpdate {
        query_id: query_id.clone(),
        changes: vec![ResultChange::FullRefresh { results: result.results.clone() }],
        trigger: UpdateTrigger::Manual { user_id },
        execution_time: result.execution_time,
        updated_at: Utc::now(),
    };
    
    query_manager.notify_subscribers(&update).await?;
    
    Ok(result)
}
```

### Svelte Real-Time Components

```svelte
<!-- QueryNode.svelte -->
<script>
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/tauri';
  import { listen } from '@tauri-apps/api/event';
  import NodeRenderer from './NodeRenderer.svelte';
  import { fade, slide } from 'svelte/transition';
  
  export let queryNodeId;
  export let queryDefinition;
  export let initialResults = [];
  
  let results = [...initialResults];
  let isLoading = false;
  let lastUpdated = null;
  let unsubscribe = null;
  let subscriberId = `ui-${Math.random().toString(36).substr(2, 9)}`;
  let updateCount = 0;
  let executionTime = null;
  
  onMount(async () => {
    try {
      // Subscribe to real-time updates
      await invoke('subscribe_to_query', {
        queryId: queryNodeId,
        subscriberId,
        subscriberType: { UIComponent: { componentId: subscriberId } }
      });
      
      // Listen for update events
      unsubscribe = await listen(`query-update`, (event) => {
        if (event.payload.query_id === queryNodeId) {
          handleQueryUpdate(event.payload);
        }
      });
      
      // Initial load if no results provided
      if (results.length === 0) {
        await refreshQuery();
      }
      
    } catch (error) {
      console.error('Failed to set up query subscription:', error);
    }
  });
  
  onDestroy(async () => {
    if (unsubscribe) {
      unsubscribe();
    }
    
    try {
      await invoke('unsubscribe_from_query', {
        queryId: queryNodeId,
        subscriberId
      });
    } catch (error) {
      console.error('Failed to unsubscribe from query:', error);
    }
  });
  
  async function refreshQuery() {
    isLoading = true;
    try {
      const result = await invoke('execute_query_manually', {
        queryId: queryNodeId,
        userId: 'current_user' // Would come from auth context
      });
      
      results = result.results;
      executionTime = result.execution_time;
      lastUpdated = new Date();
      updateCount++;
      
    } catch (error) {
      console.error('Query execution failed:', error);
    } finally {
      isLoading = false;
    }
  }
  
  function handleQueryUpdate(update) {
    console.log('Received query update:', update);
    
    // Apply incremental changes for smooth UX
    for (const change of update.changes) {
      switch (change.type) {
        case 'Added':
          // Add with animation
          results.splice(change.position, 0, change.node_id);
          break;
          
        case 'Removed':
          // Remove with animation
          results = results.filter(id => id !== change.node_id);
          break;
          
        case 'Moved':
          // Animate movement
          const [moved] = results.splice(change.from_position, 1);
          results.splice(change.to_position, 0, moved);
          break;
          
        case 'Updated':
          // Trigger re-render of specific node
          triggerNodeUpdate(change.node_id, change.changed_fields);
          break;
          
        case 'FullRefresh':
          // Complete refresh
          results = change.results;
          break;
      }
    }
    
    // Trigger reactivity
    results = [...results];
    lastUpdated = new Date();
    updateCount++;
    executionTime = update.execution_time;
    
    // Show brief notification for updates
    showUpdateNotification(update.changes.length);
  }
  
  function triggerNodeUpdate(nodeId, changedFields) {
    // Dispatch custom event for specific node updates
    window.dispatchEvent(new CustomEvent('node-update', {
      detail: { nodeId, changedFields }
    }));
  }
  
  function showUpdateNotification(changeCount) {
    // Brief visual feedback
    const notification = document.createElement('div');
    notification.className = 'update-notification';
    notification.textContent = `${changeCount} item${changeCount !== 1 ? 's' : ''} updated`;
    document.body.appendChild(notification);
    
    setTimeout(() => notification.remove(), 3000);
  }
  
  function getUpdateTriggerDescription(trigger) {
    switch (trigger.type) {
      case 'EntityChange':
        return `${trigger.entity_id} updated`;
      case 'TimeInterval':
        return 'Scheduled refresh';
      case 'Manual':
        return `Manual refresh by ${trigger.user_id}`;
      default:
        return 'Updated';
    }
  }
</script>

<div class="query-node" data-query-id={queryNodeId}>
  <div class="query-header">
    <h3>{queryDefinition.name}</h3>
    
    <div class="query-meta">
      <span class="result-count" class:loading={isLoading}>
        {isLoading ? 'Loading...' : `${results.length} result${results.length !== 1 ? 's' : ''}`}
      </span>
      
      {#if lastUpdated}
        <span class="last-updated" title="Last updated: {lastUpdated.toLocaleString()}">
          Updated {formatRelativeTime(lastUpdated)}
        </span>
      {/if}
      
      {#if executionTime}
        <span class="execution-time" title="Query execution time">
          {executionTime}ms
        </span>
      {/if}
      
      <button 
        on:click={refreshQuery} 
        disabled={isLoading}
        class="refresh-btn"
        title="Refresh query"
      >
        {isLoading ? '⟳' : '↻'}
      </button>
    </div>
  </div>
  
  <div class="query-results" class:loading={isLoading}>
    {#if isLoading && results.length === 0}
      <div class="loading-state" transition:fade>
        <div class="spinner"></div>
        <span>Loading query results...</span>
      </div>
    {:else if results.length === 0}
      <div class="empty-state" transition:fade>
        <span>No results found</span>
        <small>Try adjusting your query filters</small>
      </div>
    {:else}
      {#each results as nodeId, index (nodeId)}
        <div 
          class="result-item" 
          transition:slide|local={{ duration: 300 }}
          style="--item-index: {index}"
        >
          <NodeRenderer {nodeId} compact={true} />
        </div>
      {/each}
    {/if}
  </div>
  
  {#if updateCount > 0}
    <div class="update-indicator" transition:fade>
      <small>Live updates: {updateCount}</small>
    </div>
  {/if}
</div>

<style>
  .query-node {
    border: 2px solid #e2e4e7;
    border-radius: 8px;
    background: #f8f9fa;
    margin: 16px 0;
    overflow: hidden;
    position: relative;
  }
  
  .query-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 16px;
    background: #ffffff;
    border-bottom: 1px solid #e2e4e7;
  }
  
  .query-meta {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 0.875rem;
    color: #6b7280;
  }
  
  .result-count.loading {
    color: #3b82f6;
  }
  
  .execution-time {
    font-family: monospace;
    background: #f3f4f6;
    padding: 2px 6px;
    border-radius: 4px;
  }
  
  .refresh-btn {
    background: none;
    border: 1px solid #d1d5db;
    border-radius: 4px;
    padding: 4px 8px;
    cursor: pointer;
    font-size: 1rem;
    color: #6b7280;
    transition: all 0.2s;
  }
  
  .refresh-btn:hover:not(:disabled) {
    background: #f3f4f6;
    color: #374151;
  }
  
  .refresh-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  
  .query-results {
    max-height: 400px;
    overflow-y: auto;
    position: relative;
  }
  
  .query-results.loading {
    opacity: 0.7;
  }
  
  .result-item {
    border-bottom: 1px solid #e2e4e7;
    transition: all 0.3s ease;
    animation: slideIn 0.3s ease-out calc(var(--item-index) * 50ms);
  }
  
  .result-item:last-child {
    border-bottom: none;
  }
  
  .loading-state {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 32px;
    color: #6b7280;
  }
  
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 32px;
    color: #6b7280;
    text-align: center;
  }
  
  .update-indicator {
    position: absolute;
    top: 8px;
    right: 8px;
    background: #10b981;
    color: white;
    padding: 4px 8px;
    border-radius: 12px;
    font-size: 0.75rem;
    z-index: 10;
  }
  
  .spinner {
    width: 20px;
    height: 20px;
    border: 2px solid #e2e4e7;
    border-top: 2px solid #3b82f6;
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }
  
  @keyframes spin {
    0% { transform: rotate(0deg); }
    100% { transform: rotate(360deg); }
  }
  
  @keyframes slideIn {
    from {
      opacity: 0;
      transform: translateY(-10px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
  
  :global(.update-notification) {
    position: fixed;
    top: 20px;
    right: 20px;
    background: #10b981;
    color: white;
    padding: 8px 16px;
    border-radius: 6px;
    font-size: 0.875rem;
    z-index: 1000;
    animation: slideInNotification 0.3s ease-out, fadeOutNotification 0.3s ease-out 2.7s;
  }
  
  @keyframes slideInNotification {
    from {
      opacity: 0;
      transform: translateX(100%);
    }
    to {
      opacity: 1;
      transform: translateX(0);
    }
  }
  
  @keyframes fadeOutNotification {
    from {
      opacity: 1;
    }
    to {
      opacity: 0;
    }
  }
</style>

<script>
  function formatRelativeTime(date) {
    const now = new Date();
    const diff = now - date;
    const seconds = Math.floor(diff / 1000);
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);
    
    if (seconds < 60) return 'just now';
    if (minutes < 60) return `${minutes}m ago`;
    if (hours < 24) return `${hours}h ago`;
    return date.toLocaleDateString();
  }
</script>
```

## Performance Optimizations

### Query Execution Optimization

```rust
pub struct QueryPerformanceOptimizer {
    execution_cache: HashMap<u64, CachedQueryResult>,
    index_analyzer: IndexAnalyzer,
    cost_estimator: QueryCostEstimator,
    parallelization_planner: ParallelizationPlanner,
}

impl QueryPerformanceOptimizer {
    pub async fn optimize_query_execution(
        &self,
        query: &QueryDefinition,
        context: &QueryExecutionContext
    ) -> Result<OptimizedQueryPlan, Error> {
        // 1. Check cache first
        let cache_key = self.calculate_query_hash(query, context);
        if let Some(cached_result) = self.execution_cache.get(&cache_key) {
            if cached_result.is_still_valid(context.current_time) {
                return Ok(OptimizedQueryPlan::UseCached { 
                    cache_key,
                    estimated_time: Duration::from_millis(1),
                });
            }
        }
        
        // 2. Analyze query complexity
        let complexity_analysis = self.analyze_query_complexity(query).await?;
        
        // 3. Plan index usage
        let index_plan = self.index_analyzer.plan_index_usage(query).await?;
        
        // 4. Estimate execution cost
        let cost_estimate = self.cost_estimator.estimate_query_cost(
            query,
            &index_plan,
            &complexity_analysis
        ).await?;
        
        // 5. Plan parallelization if beneficial
        let parallel_plan = if cost_estimate.total_cost > Duration::from_millis(100) {
            Some(self.parallelization_planner.plan_parallel_execution(query).await?)
        } else {
            None
        };
        
        Ok(OptimizedQueryPlan::ExecuteOptimized {
            index_plan,
            parallel_plan,
            estimated_time: cost_estimate.total_cost,
            cache_result: cost_estimate.should_cache,
        })
    }
    
    async fn execute_optimized_query(
        &self,
        plan: OptimizedQueryPlan,
        query: &QueryDefinition,
        services: &Services
    ) -> Result<QueryExecutionResult, Error> {
        match plan {
            OptimizedQueryPlan::UseCached { cache_key, .. } => {
                let cached = self.execution_cache.get(&cache_key).unwrap();
                Ok(cached.result.clone())
            },
            
            OptimizedQueryPlan::ExecuteOptimized { 
                index_plan, 
                parallel_plan, 
                cache_result,
                .. 
            } => {
                let start_time = Instant::now();
                
                let result = if let Some(parallel_plan) = parallel_plan {
                    self.execute_parallel_query(query, &parallel_plan, services).await?
                } else {
                    self.execute_sequential_query(query, &index_plan, services).await?
                };
                
                let execution_time = start_time.elapsed();
                
                // Cache result if beneficial
                if cache_result {
                    let cache_key = self.calculate_query_hash(query, &QueryExecutionContext::current());
                    self.execution_cache.insert(cache_key, CachedQueryResult {
                        result: result.clone(),
                        cached_at: Utc::now(),
                        ttl: Duration::from_secs(300), // 5 minutes
                        dependency_hash: self.calculate_dependency_hash(query, services).await?,
                    });
                }
                
                Ok(QueryExecutionResult {
                    results: result.results,
                    metadata: result.metadata,
                    execution_time,
                    cache_hit: false,
                    stats: ExecutionStats {
                        entities_scanned: result.stats.entities_scanned,
                        filters_applied: result.stats.filters_applied,
                        index_usage: index_plan.indexes_used,
                        parallel_workers: parallel_plan.map(|p| p.worker_count).unwrap_or(1),
                    },
                    dependency_hash: self.calculate_dependency_hash(query, services).await?,
                })
            }
        }
    }
}

#[derive(Debug)]
pub enum OptimizedQueryPlan {
    UseCached { 
        cache_key: u64,
        estimated_time: Duration,
    },
    ExecuteOptimized {
        index_plan: IndexUsagePlan,
        parallel_plan: Option<ParallelExecutionPlan>,
        estimated_time: Duration,
        cache_result: bool,
    },
}

#[derive(Debug)]
pub struct ParallelExecutionPlan {
    worker_count: usize,
    data_partitions: Vec<DataPartition>,
    merge_strategy: MergeStrategy,
}

#[derive(Debug)]
pub enum MergeStrategy {
    Simple,      // Just concatenate results
    Sorted,      // Merge sort results
    Deduplicated, // Remove duplicates during merge
    Aggregated,  // Perform aggregations during merge
}
```

### Change Detection Optimization

```rust
pub struct ChangeDetectionOptimizer {
    bloom_filters: HashMap<String, BloomFilter>,
    change_frequency_tracker: ChangeFrequencyTracker,
    dependency_optimizer: DependencyOptimizer,
}

impl ChangeDetectionOptimizer {
    pub fn should_check_query(&self, query_id: &str, change_event: &EntityChangeEvent) -> bool {
        // 1. Quick bloom filter check
        if let Some(bloom_filter) = self.bloom_filters.get(query_id) {
            // If bloom filter says "definitely not affected", skip
            if !bloom_filter.might_contain(&change_event.entity_id) &&
               !change_event.changed_fields.iter().any(|field| bloom_filter.might_contain(field)) {
                return false;
            }
        }
        
        // 2. Check change frequency patterns
        if !self.change_frequency_tracker.is_change_significant(query_id, change_event) {
            return false;
        }
        
        // 3. More expensive dependency check
        self.dependency_optimizer.query_depends_on_change(query_id, change_event)
    }
    
    pub fn update_bloom_filters(&mut self, query_id: &str, query: &QueryDefinition) {
        let mut bloom_filter = BloomFilter::new(1000, 0.01); // 1000 items, 1% false positive rate
        
        // Add entity types
        for entity_type in &query.entity_types {
            bloom_filter.insert(entity_type);
        }
        
        // Add field names from filters
        for filter in &query.filters {
            bloom_filter.insert(&filter.field_name);
        }
        
        // Add sort fields
        for sort in &query.sort_by {
            bloom_filter.insert(&sort.field);
        }
        
        self.bloom_filters.insert(query_id.to_string(), bloom_filter);
    }
}

pub struct ChangeFrequencyTracker {
    change_patterns: HashMap<String, ChangePattern>,
    noise_threshold: f64,
}

#[derive(Debug)]
struct ChangePattern {
    recent_changes: VecDeque<DateTime<Utc>>,
    change_rate: f64,           // Changes per minute
    noise_level: f64,           // How often changes don't affect results
    last_significant_change: DateTime<Utc>,
}

impl ChangeFrequencyTracker {
    pub fn is_change_significant(&mut self, query_id: &str, change_event: &EntityChangeEvent) -> bool {
        let pattern = self.change_patterns.entry(query_id.to_string())
            .or_insert_with(|| ChangePattern {
                recent_changes: VecDeque::new(),
                change_rate: 0.0,
                noise_level: 0.0,
                last_significant_change: Utc::now(),
            });
        
        // Record this change
        pattern.recent_changes.push_back(change_event.timestamp);
        
        // Remove old changes (older than 1 hour)
        let cutoff = Utc::now() - chrono::Duration::hours(1);
        while let Some(&front_time) = pattern.recent_changes.front() {
            if front_time < cutoff {
                pattern.recent_changes.pop_front();
            } else {
                break;
            }
        }
        
        // Calculate current change rate
        pattern.change_rate = pattern.recent_changes.len() as f64 / 60.0; // per minute
        
        // If change rate is very high, apply noise filtering
        if pattern.change_rate > 10.0 { // More than 10 changes per minute
            // Only process if it's been at least X seconds since last significant change
            let time_since_last = (change_event.timestamp - pattern.last_significant_change)
                .num_seconds() as f64;
            
            let min_interval = (pattern.noise_level * 30.0).max(1.0); // At least 1 second
            
            if time_since_last < min_interval {
                return false; // Skip this change as noise
            }
        }
        
        pattern.last_significant_change = change_event.timestamp;
        true
    }
}
```

## Testing Strategy

### Real-Time System Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;
    
    #[tokio::test]
    async fn test_query_real_time_updates() {
        let services = create_test_services().await;
        let mut query_manager = QueryResultManager::new(services.clone());
        
        // Create a test query for tasks due this week
        let query_node = create_test_query_node("tasks_due_this_week", QueryDefinition {
            name: "Tasks Due This Week".to_string(),
            entity_types: vec!["task".to_string()],
            filters: vec![
                QueryFilter {
                    field_name: "due_date".to_string(),
                    operator: FilterOperator::Between,
                    value: QueryValue::Dynamic("THIS_WEEK".to_string()),
                    case_sensitive: false,
                    negate: false,
                },
                QueryFilter {
                    field_name: "status".to_string(),
                    operator: FilterOperator::NotEquals,
                    value: QueryValue::Static(EntityValue::Text("completed".to_string())),
                    case_sensitive: false,
                    negate: false,
                }
            ],
            sort_by: vec![SortCriteria {
                field: "due_date".to_string(),
                direction: SortDirection::Ascending,
            }],
            // ... other fields
        });
        
        // Register the query
        query_manager.register_query(&query_node, &services).await?;
        
        // Set up update listener
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        
        // Add a test subscriber
        let subscriber = QuerySubscriber {
            subscriber_id: "test_subscriber".to_string(),
            subscriber_type: SubscriberType::UIComponent { 
                component_id: "test_component".to_string() 
            },
            notification_preferences: NotificationPreferences::default(),
            last_notified: Utc::now(),
        };
        
        query_manager.add_subscriber(&query_node.id, subscriber).await?;
        
        // Create a new task that should appear in the query
        let new_task = EntityNode {
            entity_type: "task".to_string(),
            stored_fields: hashmap! {
                "title" => EntityValue::Text("Test Task".to_string()),
                "due_date" => EntityValue::Date(Utc::now() + chrono::Duration::days(2)),
                "status" => EntityValue::Text("pending".to_string()),
            },
            // ... other fields
        };
        
        // Create change event
        let change_event = EntityChangeEvent {
            entity_id: new_task.base.id.clone(),
            entity_type: "task".to_string(),
            changed_fields: vec!["title".to_string(), "due_date".to_string(), "status".to_string()],
            change_type: ChangeType::Create,
            old_values: HashMap::new(),
            new_values: new_task.stored_fields.clone(),
            user_id: "test_user".to_string(),
            timestamp: Utc::now(),
            transaction_id: Some("test_transaction".to_string()),
        };
        
        // Trigger the change
        let updates = query_manager.handle_entity_change(change_event, &services).await?;
        
        // Verify update was generated
        assert_eq!(updates.len(), 1);
        let update = &updates[0];
        assert_eq!(update.query_id, query_node.id);
        assert_eq!(update.changes.len(), 1);
        
        match &update.changes[0] {
            ResultChange::Added { node_id, position, .. } => {
                assert_eq!(node_id, &new_task.base.id);
                assert_eq!(*position, 0); // Should be first due to sort order
            },
            _ => panic!("Expected Added change"),
        }
    }
    
    #[tokio::test]
    async fn test_query_performance_optimization() {
        let services = create_test_services().await;
        let optimizer = QueryPerformanceOptimizer::new();
        
        // Create a complex query
        let complex_query = QueryDefinition {
            name: "Complex Employee Analysis".to_string(),
            entity_types: vec!["employee".to_string()],
            filters: vec![
                QueryFilter {
                    field_name: "department".to_string(),
                    operator: FilterOperator::In(vec![
                        EntityValue::Text("Engineering".to_string()),
                        EntityValue::Text("Product".to_string()),
                    ]),
                    value: QueryValue::Static(EntityValue::Text("".to_string())),
                    case_sensitive: false,
                    negate: false,
                },
                QueryFilter {
                    field_name: "salary".to_string(),
                    operator: FilterOperator::GreaterThan,
                    value: QueryValue::Static(EntityValue::Number(50000.0)),
                    case_sensitive: false,
                    negate: false,
                },
            ],
            aggregations: vec![
                AggregationDefinition {
                    function: AggregationFunction::Average,
                    field: "salary".to_string(),
                    alias: "avg_salary".to_string(),
                }
            ],
            // ... other fields
        };
        
        let context = QueryExecutionContext::current();
        let plan = optimizer.optimize_query_execution(&complex_query, &context).await?;
        
        // Should generate an optimized plan
        match plan {
            OptimizedQueryPlan::ExecuteOptimized { estimated_time, .. } => {
                assert!(estimated_time < Duration::from_millis(500));
            },
            OptimizedQueryPlan::UseCached { .. } => {
                // Cache hit is also valid
            }
        }
    }
    
    #[tokio::test]
    async fn test_change_detection_optimization() {
        let mut optimizer = ChangeDetectionOptimizer::new();
        
        // Set up a query and its bloom filter
        let query_id = "test_query";
        let query = QueryDefinition {
            name: "Employee Query".to_string(),
            entity_types: vec!["employee".to_string()],
            filters: vec![
                QueryFilter {
                    field_name: "department".to_string(),
                    operator: FilterOperator::Equals,
                    value: QueryValue::Static(EntityValue::Text("Engineering".to_string())),
                    case_sensitive: false,
                    negate: false,
                }
            ],
            // ... other fields
        };
        
        optimizer.update_bloom_filters(query_id, &query);
        
        // Test that unrelated changes are filtered out
        let unrelated_change = EntityChangeEvent {
            entity_id: "customer_123".to_string(),
            entity_type: "customer".to_string(),
            changed_fields: vec!["email".to_string()],
            change_type: ChangeType::Update,
            old_values: hashmap! {
                "email" => EntityValue::Text("old@example.com".to_string()),
            },
            new_values: hashmap! {
                "email" => EntityValue::Text("new@example.com".to_string()),
            },
            user_id: "test_user".to_string(),
            timestamp: Utc::now(),
            transaction_id: None,
        };
        
        assert!(!optimizer.should_check_query(query_id, &unrelated_change));
        
        // Test that related changes are detected
        let related_change = EntityChangeEvent {
            entity_id: "employee_456".to_string(),
            entity_type: "employee".to_string(),
            changed_fields: vec!["department".to_string()],
            change_type: ChangeType::Update,
            old_values: hashmap! {
                "department" => EntityValue::Text("Sales".to_string()),
            },
            new_values: hashmap! {
                "department" => EntityValue::Text("Engineering".to_string()),
            },
            user_id: "test_user".to_string(),
            timestamp: Utc::now(),
            transaction_id: None,
        };
        
        assert!(optimizer.should_check_query(query_id, &related_change));
    }
    
    #[tokio::test]
    async fn test_concurrent_query_updates() {
        let services = create_test_services().await;
        let mut query_manager = QueryResultManager::new(services.clone());
        
        // Create multiple queries
        let query_nodes = (0..5).map(|i| {
            create_test_query_node(&format!("query_{}", i), create_test_query_definition())
        }).collect::<Vec<_>>();
        
        // Register all queries
        for query_node in &query_nodes {
            query_manager.register_query(query_node, &services).await?;
        }
        
        // Generate many concurrent changes
        let change_tasks: Vec<_> = (0..100).map(|i| {
            let services = services.clone();
            let change_event = create_test_change_event(&format!("entity_{}", i));
            
            tokio::spawn(async move {
                let mut query_manager = services.query_manager.lock().await;
                query_manager.handle_entity_change(change_event, &services).await
            })
        }).collect();
        
        // Wait for all changes to complete
        let results = futures::future::join_all(change_tasks).await;
        
        // Verify all changes processed successfully
        for result in results {
            let change_result = result??;
            // All changes should complete without errors
        }
        
        // Verify final consistency
        for query_node in &query_nodes {
            let active_query = query_manager.active_queries.get(&query_node.id).unwrap();
            // Results should be consistent with all applied changes
            assert!(!active_query.current_results.is_empty());
        }
    }
    
    #[tokio::test]
    async fn test_subscription_management() {
        let services = create_test_services().await;
        let mut query_manager = QueryResultManager::new(services.clone());
        
        let query_node = create_test_query_node("test_query", create_test_query_definition());
        query_manager.register_query(&query_node, &services).await?;
        
        // Add multiple subscribers
        let subscribers = (0..10).map(|i| QuerySubscriber {
            subscriber_id: format!("subscriber_{}", i),
            subscriber_type: SubscriberType::UIComponent { 
                component_id: format!("component_{}", i) 
            },
            notification_preferences: NotificationPreferences::default(),
            last_notified: Utc::now(),
        }).collect::<Vec<_>>();
        
        for subscriber in &subscribers {
            query_manager.add_subscriber(&query_node.id, subscriber.clone()).await?;
        }
        
        // Verify all subscribers were added
        let active_query = query_manager.active_queries.get(&query_node.id).unwrap();
        assert_eq!(active_query.subscribers.len(), 10);
        
        // Remove some subscribers
        for i in 0..5 {
            query_manager.remove_subscriber(&query_node.id, &format!("subscriber_{}", i)).await?;
        }
        
        // Verify subscribers were removed
        let active_query = query_manager.active_queries.get(&query_node.id).unwrap();
        assert_eq!(active_query.subscribers.len(), 5);
        
        // Generate a change and verify remaining subscribers are notified
        let change_event = create_test_change_event("test_entity");
        let updates = query_manager.handle_entity_change(change_event, &services).await?;
        
        assert!(!updates.is_empty());
        // Notification testing would require additional mock infrastructure
    }
}

fn create_test_query_node(id: &str, query: QueryDefinition) -> QueryNode {
    QueryNode {
        base: TextNode {
            id: id.to_string(),
            content: query.name.clone(),
            parent_id: None,
            children: Vec::new(),
            metadata: HashMap::new(),
            created_at: Utc::now(),
            modified_at: Utc::now(),
        },
        query,
        view_calculated_fields: Vec::new(),
        results: Vec::new(),
        last_executed: Utc::now(),
        auto_refresh: true,
        refresh_triggers: vec![
            RefreshTrigger::FieldChange { 
                entity_type: "task".to_string(), 
                field: "status".to_string(),
                condition: None,
            }
        ],
        subscription_id: uuid::Uuid::new_v4().to_string(),
        performance_metrics: QueryPerformanceMetrics::default(),
    }
}
```

---

This Real-Time Architecture specification provides a comprehensive framework for live data synchronization in NodeSpace, with sophisticated query management, event-driven updates, performance optimization, and robust testing strategies. The system ensures that all user interfaces stay synchronized with minimal latency while maintaining high performance through intelligent caching and change detection.