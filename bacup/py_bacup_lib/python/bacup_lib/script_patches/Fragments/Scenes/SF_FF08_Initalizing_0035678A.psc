Function Fragment_Phase_01_Begin()
    Actor pharmabotRef = Pharmabot.GetActorReference()
    If pharmabotRef != None && InitIdle != None
        pharmabotRef.PlayIdle(InitIdle)
    EndIf
EndFunction
