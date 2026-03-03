local M = {}

local state = require("docent.state")

local pipe = nil
local job_id = nil
local request_id = 0
local pending_callbacks = {}
local notification_handlers = {}
local line_buffer = ""

local function on_data(data)
  for _, chunk in ipairs(data) do
    line_buffer = line_buffer .. chunk
  end

  -- Process complete JSON lines
  while true do
    local newline_pos = line_buffer:find("\n")
    if not newline_pos then
      break
    end

    local line = line_buffer:sub(1, newline_pos - 1)
    line_buffer = line_buffer:sub(newline_pos + 1)

    if line ~= "" then
      local ok, msg = pcall(vim.json.decode, line)
      if ok and msg then
        if msg.id and pending_callbacks[msg.id] then
          -- Response to a request
          local cb = pending_callbacks[msg.id]
          pending_callbacks[msg.id] = nil
          vim.schedule(function()
            cb(msg.error, msg.result)
          end)
        elseif msg.method then
          -- Notification
          vim.schedule(function()
            local handler = notification_handlers[msg.method]
            if handler then
              handler(msg.params)
            end
          end)
        end
      end
    end
  end
end

function M.on_notification(method, handler)
  notification_handlers[method] = handler
end

function M.send(method, params, callback)
  if not pipe then
    if callback then
      callback("not connected", nil)
    end
    return
  end

  request_id = request_id + 1
  local id = request_id

  local req = { id = id, method = method }
  if params then
    req.params = params
  end

  if callback then
    pending_callbacks[id] = callback
  end

  local json = vim.json.encode(req) .. "\n"
  pipe:write(json)
end

function M.start(args, callbacks)
  callbacks = callbacks or {}

  -- Build the command
  local cmd = { "docent", "--headless" }
  for _, arg in ipairs(args) do
    table.insert(cmd, arg)
  end

  local socket_path = nil

  job_id = vim.fn.jobstart(cmd, {
    on_stdout = function(_, data, _)
      -- First line of stdout is the socket path
      if not socket_path then
        for _, line in ipairs(data) do
          line = vim.trim(line)
          if line ~= "" then
            socket_path = line
            -- Connect to the socket
            vim.schedule(function()
              M._connect(socket_path, callbacks)
            end)
            return
          end
        end
      end
    end,
    on_stderr = function(_, data, _)
      local msg = table.concat(data, "\n")
      if msg ~= "" and callbacks.on_error then
        vim.schedule(function()
          callbacks.on_error(msg)
        end)
      end
    end,
    on_exit = function(_, code, _)
      pipe = nil
      job_id = nil
      if callbacks.on_exit then
        vim.schedule(function()
          callbacks.on_exit(code)
        end)
      end
    end,
  })

  if job_id <= 0 then
    if callbacks.on_error then
      callbacks.on_error("failed to start docent process")
    end
    return false
  end

  return true
end

function M._connect(socket_path, callbacks)
  pipe = vim.uv.new_pipe(false)
  pipe:connect(socket_path, function(err)
    if err then
      vim.schedule(function()
        if callbacks.on_error then
          callbacks.on_error("failed to connect to socket: " .. err)
        end
      end)
      return
    end

    pipe:read_start(function(read_err, data)
      if read_err then
        vim.schedule(function()
          if callbacks.on_error then
            callbacks.on_error("read error: " .. read_err)
          end
        end)
        return
      end

      if data then
        on_data({ data })
      end
    end)

    vim.schedule(function()
      if callbacks.on_connect then
        callbacks.on_connect()
      end
    end)
  end)
end

function M.stop()
  M.send("shutdown", nil, nil)

  vim.defer_fn(function()
    if pipe then
      pipe:read_stop()
      pipe:close()
      pipe = nil
    end
    if job_id then
      vim.fn.jobstop(job_id)
      job_id = nil
    end
    pending_callbacks = {}
    line_buffer = ""
  end, 100)
end

function M.is_connected()
  return pipe ~= nil
end

return M
