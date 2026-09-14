Function Fragment_Phase_01_End()
    Actor vertibot = Alias_AirDropVertibot.GetActorReference()
    If vertibot != None && VertibirdLand != None
        vertibot.SetValue(VertibirdLand, 1.0)
        vertibot.EvaluatePackage()
    EndIf
EndFunction
