Function PublishSprayedLocations()
    B21:QuestVariables questVariables = (Self as Quest) as B21:QuestVariables
    If questVariables != None
        questVariables.SetVariable("SprayLocCount", Count as Float)
    EndIf
EndFunction

Function ResetSprayedLocations()
    Count = 0
    PublishSprayedLocations()
EndFunction

Function AddSprayedLocation()
    Count += 1
    PublishSprayedLocations()
EndFunction

Int Function GetSprayedLocations()
    Return Count
EndFunction

Bool Function EventStillRunning()
    If !IsRunning()
        Return False
    EndIf
    Return !IsStageDone(55) && !IsStageDone(90) && !IsStageDone(97) && !IsStageDone(98) && !IsStageDone(99) && !IsStageDone(100)
EndFunction

Event OnQuestInit()
    ResetSprayedLocations()
    B21:QuestTimer questTimer = (Self as Quest) as B21:QuestTimer
    If questTimer != None
        RegisterForCustomEvent(questTimer, "QuestTimerEnded")
    EndIf
EndEvent

Event B21:QuestTimer.QuestTimerEnded(B21:QuestTimer akSender, Var[] akArgs)
    If QuestTimerEndStage < 0 || !EventStillRunning() || IsStageDone(QuestTimerEndStage)
        Return
    EndIf
    SetStage(QuestTimerEndStage)
EndEvent

Event OnQuestShutdown()
    UnregisterForAllEvents()
EndEvent
