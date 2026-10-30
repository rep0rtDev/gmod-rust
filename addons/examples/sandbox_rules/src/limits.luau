--!strict
-- The sandbox prop limit as an addon: a spawn is refused once the player's live props reach
-- sbox_maxprops (the game's Player:CheckLimit("props"): refused when count > limit - 1; a
-- negative limit means unlimited). Runs before the Rust sandbox's own check at priority 0.
state.hits = state.hits or 0

local function live_props(player: number): number
    local n = 0
    for _, e in ipairs(world.find()) do
        if e.owner == player then
            n += 1
        end
    end
    return n
end

events.on("prop.spawn_request", function(p)
    local limit = cvar.get_number("sbox_maxprops") or 200
    if limit >= 0 and live_props(p.player) > limit - 1 then
        state.hits += 1
        state.last_refused = { player = p.player, model = p.model, tick = game.tick() }
        return false
    end
    return nil
end)
