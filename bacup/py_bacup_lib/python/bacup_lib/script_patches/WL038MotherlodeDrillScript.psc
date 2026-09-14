Function ClientShakeCameraAndController()
    If Game.GetPlayer().GetDistance(Self as ObjectReference) < 2000.0
        Game.ShakeCamera(Self as ObjectReference, 0.5, 3.0)
        Game.ShakeController(0.5, 0.5, 3.0)
    EndIf
EndFunction
