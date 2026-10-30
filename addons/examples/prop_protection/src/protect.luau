--!strict
-- Prop protection: a player may touch only their own props. The game itself has none of this
-- (anyone physguns, tools or edits anyone's prop); this is what protection addons add on top,
-- built on the entity's `owner` in the snapshot (0: the world or the server, left alone).
-- Vetoes the physgun pickup, the tool gun click, context-menu properties and the +use pickup
-- on another player's prop; counts the blocks per player and reports them on each spawn.
state.blocked = state.blocked or {}
state.by_hook = state.by_hook or {}

local function blocked_for(player: number): number
    local n = state.blocked[tostring(player)]
    return if n == nil then 0 else n
end

local function owned_by_other(player: number, entity: number): boolean
    if entity == 0 then
        return false
    end
    local e = world.get(entity)
    if e == nil or e.owner == 0 or e.owner == player then
        return false
    end
    return true
end

local function guard(hook: string, player: number, entity: number)
    if owned_by_other(player, entity) then
        state.blocked[tostring(player)] = blocked_for(player) + 1
        state.by_hook[hook] = (state.by_hook[hook] or 0) + 1
        return false
    end
    return nil
end

events.on("physgun.pickup_request", function(e)
    return guard("physgun", e.player, e.entity)
end)

events.on("tool.before", function(e)
    return guard("tool", e.player, e.hit.ent)
end)

events.on("player.property_request", function(e)
    return guard("property", e.player, e.entity)
end)

events.on("player.pickup_request", function(e)
    return guard("use", e.player, e.entity)
end)

events.on("player.spawned", function(e)
    local h = state.by_hook
    net.send("prop_protection", {
        blocked = blocked_for(e.player),
        physgun = h.physgun or 0,
        tool = h.tool or 0,
        property = h.property or 0,
        use = h.use or 0,
    }, e.player)
end)
