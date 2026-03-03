local M = {}

local client = require("docent.client")
local state = require("docent.state")
local minimap = require("docent.minimap")
local diff = require("docent.diff")

local config = {
  minimap_width = 35,
}

local function setup_highlights()
  vim.api.nvim_set_hl(0, "DocentStepCurrent", { fg = "#61afef", bold = true, default = true })
  vim.api.nvim_set_hl(0, "DocentStepCompleted", { fg = "#98c379", default = true })
  vim.api.nvim_set_hl(0, "DocentStepPending", { fg = "#5c6370", default = true })
end

local function setup_notification_handlers()
  client.on_notification("state_snapshot", function(params)
    state.handle_state_snapshot(params)
  end)

  client.on_notification("state_changed", function(params)
    state.handle_state_changed(params)
  end)

  client.on_notification("step_changed", function(params)
    state.handle_step_changed(params)
  end)

  client.on_notification("walkthrough_loaded", function(params)
    state.handle_walkthrough_loaded(params)
  end)

  client.on_notification("rechunk_complete", function(params)
    state.handle_rechunk_complete(params)
  end)

  client.on_notification("chat_chunk", function(params)
    state.chat_pending = params and params.step_index
  end)

  client.on_notification("chat_complete", function(params)
    state.chat_pending = nil
  end)

  client.on_notification("error", function(params)
    if params and params.message then
      vim.notify("[docent] " .. params.message, vim.log.levels.ERROR)
    end
  end)

  client.on_notification("shutdown", function()
    M.stop()
  end)
end

local function setup_refresh()
  state.on_refresh(function(reason)
    if minimap.is_open() then
      minimap.render()
    end
    if diff.is_open() then
      diff.render()
    end
  end)
end

local function setup_keymaps()
  -- Global docent keymaps (active when docent is running)
  vim.keymap.set("n", "]s", function()
    if client.is_connected() then
      client.send("navigate", { action = "next" })
    end
  end, { desc = "Docent: next step" })

  vim.keymap.set("n", "[s", function()
    if client.is_connected() then
      client.send("navigate", { action = "prev" })
    end
  end, { desc = "Docent: previous step" })

  vim.keymap.set("n", "<leader>dc", function()
    if client.is_connected() then
      client.send("complete_step")
    end
  end, { desc = "Docent: complete step" })

  vim.keymap.set("n", "<leader>dx", function()
    if client.is_connected() then
      client.send("toggle_reviewed")
    end
  end, { desc = "Docent: toggle reviewed" })

  vim.keymap.set("n", "<leader>dr", function()
    if client.is_connected() then
      client.send("rechunk")
    end
  end, { desc = "Docent: rechunk step" })
end

function M.start(input)
  if client.is_connected() then
    vim.notify("[docent] Already running. Use :DocentStop first.", vim.log.levels.WARN)
    return
  end

  if not input or input == "" then
    vim.notify("[docent] Usage: :Docent <file_or_url>", vim.log.levels.ERROR)
    return
  end

  state.reset()
  setup_highlights()
  setup_notification_handlers()
  setup_refresh()

  local started = client.start({ input }, {
    on_connect = function()
      vim.notify("[docent] Connected", vim.log.levels.INFO)
      minimap.open(config.minimap_width)
      diff.open()
    end,
    on_error = function(msg)
      vim.notify("[docent] Error: " .. msg, vim.log.levels.ERROR)
    end,
    on_exit = function(code)
      if code ~= 0 then
        vim.notify("[docent] Process exited with code " .. code, vim.log.levels.WARN)
      end
      minimap.close()
      diff.close()
      state.reset()
    end,
  })

  if started then
    setup_keymaps()
  end
end

function M.stop()
  client.stop()
  minimap.close()
  diff.close()
  state.reset()
  vim.notify("[docent] Stopped", vim.log.levels.INFO)
end

function M.setup(opts)
  opts = opts or {}

  if opts.minimap_width then
    config.minimap_width = opts.minimap_width
  end

  vim.api.nvim_create_user_command("Docent", function(cmd)
    M.start(cmd.args)
  end, {
    nargs = 1,
    complete = "file",
    desc = "Start a docent walkthrough",
  })

  vim.api.nvim_create_user_command("DocentStop", function()
    M.stop()
  end, {
    desc = "Stop the docent walkthrough",
  })
end

return M
