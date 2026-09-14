Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 200
        RegisterForPlayerLoadReconciliation()
        EnsureAttackTimer()
    ElseIf auiStageID == 255
        CancelTimer(AttackTimerID)
    EndIf
EndEvent

Event OnQuestInit()
    RegisterForPlayerLoadReconciliation()
EndEvent

Event OnQuestShutdown()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
    CancelTimer(AttackTimerID)
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        EnsureAttackTimer()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    Quest earthQuest = Self as Quest
    If aiTimerID == AttackTimerID && earthQuest.IsRunning() && earthQuest.IsStageDone(200) && !earthQuest.IsStageDone(255)
        earthQuest.SetStage(255)
    EndIf
EndEvent

Function RegisterForPlayerLoadReconciliation()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
EndFunction

Function EnsureAttackTimer()
    Quest earthQuest = Self as Quest
    If !earthQuest.IsRunning() || !earthQuest.IsStageDone(200) || earthQuest.IsStageDone(255)
        Return
    EndIf

    GlobalVariable wheelTime = Game.GetFormFromFile(0x003DA814, "SeventySix.esm") as GlobalVariable
    Float duration = 1800.0
    If wheelTime != None && wheelTime.GetValue() > 0.0
        duration = wheelTime.GetValue()
    EndIf
    CancelTimer(AttackTimerID)
    StartTimer(duration, AttackTimerID)
EndFunction
