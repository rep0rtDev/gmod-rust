--!strict
-- A client-realm HUD: a bar with the prop count and the server tick, top right, in a 24 px
-- Roboto Bold with the game's sprint icon in front of it.
draw.font("counter", { family = "Roboto Bold", size = 24 })
hud.register("counter", function(s)
    local props = world.count()
    local text = string.format("props %d  tick %d", props, game.tick())
    local tw, th = draw.text_size(text, "counter")
    local w = tw + 16 + 32
    draw.rect(s.w - w - 8, 8, w, th + 8, { r = 0, g = 0, b = 0, a = 140 })
    draw.texture(s.w - w, 8 + (th + 8 - 24) / 2, 24, 24, "vgui/hud/icon_locked")
    draw.text(s.w - w + 32, 12, text, { r = 120, g = 200, b = 255 }, "counter")
end)
