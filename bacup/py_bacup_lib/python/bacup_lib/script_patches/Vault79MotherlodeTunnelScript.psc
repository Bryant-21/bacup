Function ClientCameraShake()
    sEarthquakeSound.Play(Self as ObjectReference)
    Game.ShakeCamera(Game.GetPlayer(), 0.25, 3.0)
    Game.ShakeController(0.25, 0.25, 3.0)
EndFunction
