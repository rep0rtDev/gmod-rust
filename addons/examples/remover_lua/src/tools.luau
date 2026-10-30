--!strict
-- The remover stool: a left click removes the prop under the crosshair (the world is left
-- alone), right click and reload do nothing. No undo entry, like the game's remover.
tool.register("remover_lua", {
    left = function(ctx)
        if ctx.hit.ent == 0 then
            return nil
        end
        return { { op = "remove" } }
    end,
})
