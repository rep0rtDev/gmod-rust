--!strict
-- The weld stool written against the tool API, with the rules measured on the real gamemode:
-- first click stores the object (stage 1), second click welds object 1 to the hit and clears;
-- the world is never object 1 twice (constraint.Weld swaps it), reload clears.
tool.register("weld_lua", {
    -- the client shows these in the tool panel and sends them with every click (ctx.settings)
    settings = {
        { name = "forcelimit", type = "number", min = 0, max = 10000, default = 0 },
        { name = "nocollide", type = "bool", default = false },
    },
    left = function(ctx)
        if #ctx.objects == 0 then
            return { { op = "set_object" } }
        end
        local ent1 = ctx.objects[1].ent
        local ent2 = ctx.hit.ent
        local forcelimit = tonumber(ctx.settings.forcelimit) or 0
        local nocollide = ctx.settings.nocollide == "1"
        return {
            { op = "weld", ent1 = ent1, ent2 = ent2, forcelimit = forcelimit, nocollide = nocollide, undo = "Weld" },
            { op = "clear" },
        }
    end,
    reload = function(ctx)
        return { { op = "clear" } }
    end,
})

-- rope and axis: two clicks store the objects, the second finishes the constraint with the
-- measured rules (length between the stored points; axis alignment and hinge).
for _, name in ipairs({ "rope", "axis" }) do
    tool.register(name .. "_lua", {
        left = function(ctx)
            if name == "axis" and #ctx.objects == 0 and ctx.hit.ent == 0 then
                return nil
            end
            if #ctx.objects == 0 then
                return { { op = "set_object" } }
            end
            return { { op = "set_object" }, { op = name }, { op = "clear" } }
        end,
        reload = function(ctx)
            return { { op = "clear" } }
        end,
    })
end
