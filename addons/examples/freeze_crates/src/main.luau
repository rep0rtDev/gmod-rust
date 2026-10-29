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

-- the floor under the first player, by a trace through the world and the props
events.on("tick.post", function()
    local players = world.players()
    if #players > 0 then
        local o = players[1].origin
        local tr = phys.trace({ x = o.x, y = o.y, z = o.z + 10 }, { x = o.x, y = o.y, z = o.z - 100 })
        if tr.hit then
            state.floor_z = tr.pos.z
            state.floor_ent = tr.ent
        end
    end
end)
