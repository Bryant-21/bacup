Function BeginPerformanceDucking()
    If CSDuckMusic != None
        CSDuckMusic.Push(1.0)
    EndIf
EndFunction

Function EndPerformanceDucking()
    If CSDuckMusic != None
        CSDuckMusic.Remove()
    EndIf
EndFunction

Function RestoreQuestActors()
    Actor vin = Alias_Actor_Vin.GetActorReference()
    If vin != None
        vin.Enable()
        vin.EvaluatePackage()
    EndIf
EndFunction

Event OnQuestInit()
    RestoreQuestActors()
EndEvent

Event OnQuestShutdown()
    EndPerformanceDucking()
EndEvent
