local M = {}

local state = require("docent.state")
local client = require("docent.client")

local buf = nil
local win = nil

local function is_last_child(steps, index)
  local depth = steps[index].depth or 0
  if index >= #steps then
    return true
  end
  local next_depth = steps[index + 1].depth or 0
  return next_depth < depth
end

function M.render()
  if not buf or not vim.api.nvim_buf_is_valid(buf) then
    return
  end

  local steps = state.walkthrough and state.walkthrough.steps or {}
  local visited = state.visited or {}
  local current = state.current_step -- 0-indexed from server

  local reviewed_lines = 0
  local total_lines = 0

  for i, step in ipairs(steps) do
    local step_lines = 0
    for _, hunk in ipairs(step.hunks or {}) do
      local content = hunk.content or ""
      for _ in content:gmatch("[^\n]*") do
        step_lines = step_lines + 1
      end
    end
    total_lines = total_lines + step_lines
    -- visited is 0-indexed from server, but Lua table is 1-indexed
    if visited[i] then
      reviewed_lines = reviewed_lines + step_lines
    end
  end

  local title = string.format(
    "Steps (%d/%d) · %d/%d lines",
    current + 1,
    #steps,
    reviewed_lines,
    total_lines
  )

  local lines = { title, string.rep("─", 35) }
  local highlights = {}

  for i, step in ipairs(steps) do
    local is_current = (i - 1) == current
    local is_visited = visited[i] or false
    local depth = step.depth or 0
    local line

    if depth > 0 then
      local indent = string.rep("  ", depth - 1)
      local branch = is_last_child(steps, i) and "└── " or "├── "
      line = indent .. branch .. (step.title or "")
    else
      local indicator = is_visited and "✓" or "○"
      line = indicator .. " " .. (step.title or "")
    end

    if is_current then
      line = line .. " ←"
    end

    table.insert(lines, line)

    local hl_group
    if is_current then
      hl_group = "DocentStepCurrent"
    elseif is_visited then
      hl_group = "DocentStepCompleted"
    else
      hl_group = "DocentStepPending"
    end

    table.insert(highlights, {
      line = #lines - 1, -- 0-indexed
      group = hl_group,
    })
  end

  vim.api.nvim_buf_set_option(buf, "modifiable", true)
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
  vim.api.nvim_buf_set_option(buf, "modifiable", false)

  -- Apply highlights
  vim.api.nvim_buf_clear_namespace(buf, -1, 0, -1)
  for _, hl in ipairs(highlights) do
    vim.api.nvim_buf_add_highlight(buf, -1, hl.group, hl.line, 0, -1)
  end

  -- Move cursor to current step (line index = current + 2 for title + separator)
  if win and vim.api.nvim_win_is_valid(win) then
    local cursor_line = current + 3 -- 1-indexed, +2 for header lines
    if cursor_line <= #lines then
      vim.api.nvim_win_set_cursor(win, { cursor_line, 0 })
    end
  end
end

function M.open(width)
  width = width or 35

  if buf and vim.api.nvim_buf_is_valid(buf) then
    if win and vim.api.nvim_win_is_valid(win) then
      return
    end
  end

  buf = vim.api.nvim_create_buf(false, true)
  vim.api.nvim_buf_set_option(buf, "buftype", "nofile")
  vim.api.nvim_buf_set_option(buf, "bufhidden", "wipe")
  vim.api.nvim_buf_set_option(buf, "filetype", "docent-minimap")

  vim.cmd("topleft " .. width .. "vnew")
  win = vim.api.nvim_get_current_win()
  vim.api.nvim_win_set_buf(win, buf)

  vim.api.nvim_win_set_option(win, "number", false)
  vim.api.nvim_win_set_option(win, "relativenumber", false)
  vim.api.nvim_win_set_option(win, "signcolumn", "no")
  vim.api.nvim_win_set_option(win, "wrap", false)
  vim.api.nvim_win_set_option(win, "cursorline", true)

  -- Keymaps
  local opts = { buffer = buf, silent = true }
  vim.keymap.set("n", "<CR>", function()
    local line = vim.api.nvim_win_get_cursor(win)[1]
    local step_index = line - 3 -- account for header lines, 0-indexed
    if step_index >= 0 then
      client.send("navigate", { action = "goto", step = step_index })
    end
  end, opts)

  vim.keymap.set("n", "x", function()
    client.send("toggle_reviewed")
  end, opts)

  M.render()
end

function M.close()
  if win and vim.api.nvim_win_is_valid(win) then
    vim.api.nvim_win_close(win, true)
  end
  win = nil
  buf = nil
end

function M.is_open()
  return win ~= nil and vim.api.nvim_win_is_valid(win)
end

return M
