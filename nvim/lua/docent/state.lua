local M = {}

M.session_state = nil
M.review_mode = nil
M.current_step = 0
M.walkthrough = nil
M.visited = {}
M.walkthrough_complete = false
M.chat_pending = nil
M.rechunk_pending = false

local refresh_callbacks = {}

function M.on_refresh(callback)
  table.insert(refresh_callbacks, callback)
end

local function notify_refresh(reason)
  for _, cb in ipairs(refresh_callbacks) do
    cb(reason)
  end
end

function M.handle_state_snapshot(params)
  if not params then return end
  M.session_state = params.state
  M.review_mode = params.review_mode
  M.current_step = params.current_step or 0
  M.walkthrough = params.walkthrough
  M.visited = params.visited or {}
  M.walkthrough_complete = params.walkthrough_complete or false
  M.chat_pending = params.chat_pending
  M.rechunk_pending = params.rechunk_pending or false
  notify_refresh("snapshot")
end

function M.handle_state_changed(params)
  if not params then return end
  M.session_state = params.state
  notify_refresh("state_changed")
end

function M.handle_step_changed(params)
  if not params then return end
  M.current_step = params.current_step or M.current_step
  M.visited = params.visited or M.visited
  M.walkthrough_complete = params.walkthrough_complete or false

  -- Update the step data in the walkthrough if we have it
  if M.walkthrough and params.step and params.current_step then
    local idx = params.current_step + 1 -- Lua is 1-indexed
    if M.walkthrough.steps and idx >= 1 and idx <= #M.walkthrough.steps then
      M.walkthrough.steps[idx] = params.step
    end
  end

  notify_refresh("step_changed")
end

function M.handle_walkthrough_loaded(params)
  if not params then return end
  M.walkthrough = params.walkthrough
  M.visited = params.visited or {}
  M.session_state = { type = "ready" }
  notify_refresh("walkthrough_loaded")
end

function M.handle_rechunk_complete(params)
  if not params then return end
  if M.walkthrough then
    M.walkthrough.steps = params.steps
  end
  M.current_step = params.current_step or M.current_step
  M.visited = params.visited or M.visited
  M.rechunk_pending = false
  notify_refresh("rechunk_complete")
end

function M.reset()
  M.session_state = nil
  M.review_mode = nil
  M.current_step = 0
  M.walkthrough = nil
  M.visited = {}
  M.walkthrough_complete = false
  M.chat_pending = nil
  M.rechunk_pending = false
end

return M
