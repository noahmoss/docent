local M = {}

local state = require("docent.state")

local buf = nil
local win = nil

function M.render()
  if not buf or not vim.api.nvim_buf_is_valid(buf) then
    return
  end

  local step
  if state.walkthrough and state.walkthrough.steps then
    step = state.walkthrough.steps[state.current_step + 1] -- 1-indexed
  end

  local lines = {}

  if not step then
    lines = { "No diff content" }
  else
    for _, hunk in ipairs(step.hunks or {}) do
      -- File header
      table.insert(lines, string.format("─── %s ───", hunk.file_path or ""))
      table.insert(lines, "")

      -- Diff content
      for line in (hunk.content or ""):gmatch("[^\n]*") do
        table.insert(lines, line)
      end
      table.insert(lines, "")
    end
  end

  vim.api.nvim_buf_set_option(buf, "modifiable", true)
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
  vim.api.nvim_buf_set_option(buf, "modifiable", false)
end

function M.open()
  if buf and vim.api.nvim_buf_is_valid(buf) then
    if win and vim.api.nvim_win_is_valid(win) then
      return
    end
  end

  buf = vim.api.nvim_create_buf(false, true)
  vim.api.nvim_buf_set_option(buf, "buftype", "nofile")
  vim.api.nvim_buf_set_option(buf, "bufhidden", "wipe")
  vim.api.nvim_buf_set_option(buf, "filetype", "diff")

  vim.cmd("vnew")
  win = vim.api.nvim_get_current_win()
  vim.api.nvim_win_set_buf(win, buf)

  vim.api.nvim_win_set_option(win, "number", false)
  vim.api.nvim_win_set_option(win, "relativenumber", false)
  vim.api.nvim_win_set_option(win, "signcolumn", "no")

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
