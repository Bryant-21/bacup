Function ClientCameraShake1()
    Game.ShakeCamera(Game.GetPlayer(), 0.25, 9.0)
    Game.ShakeController(0.25, 0.25, 9.0)
EndFunction

Function ClientCameraShake2()
    Game.ShakeCamera(Game.GetPlayer(), 0.5, 3.0)
    Game.ShakeController(0.5, 0.5, 3.0)
EndFunction
