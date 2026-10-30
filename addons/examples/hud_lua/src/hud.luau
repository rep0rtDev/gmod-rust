--!strict
-- A client-realm HUD: a bar with the prop count and the server tick, top right.
hud.register("counter", function(s)
    local props = world.count()
    local text = string.format("props %d  tick %d", props, game.tick())
    local w = 8 * #text + 16
    draw.rect(s.w - w - 8, 8, w, 24, { r = 0, g = 0, b = 0, a = 140 })
    draw.text(s.w - w - 2, 11, text, { r = 120, g = 200, b = 255 })
end)
