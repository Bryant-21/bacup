Function Fragment_End(Actor akActor)
    If akActor != None && AmbushRelease != None
        akActor.SetValue(AmbushRelease, 1.0)
    EndIf
EndFunction
