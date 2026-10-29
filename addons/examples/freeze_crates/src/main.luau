--!strict
-- Refuses cans and freezes every spawned crate the tick after it appears.
events.on("prop.spawn_request", function(p)
    if p.model == "models/props_junk/popcan01a.mdl" then
        return false
    end
    return nil
end)

events.on("entity.spawned", function(p)
    local e = world.get(p.entity)
    if e and e.model == "models/props_junk/wood_crate001a.mdl" then
        world.queue("freeze", p.entity, true)
    end
end)

events.on("tick.post", function()
    state.ticks = (state.ticks or 0) + 1
end)
