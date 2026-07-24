Event OnQuestInit()
    Actor player = Game.GetPlayer()
    If player
        RegisterForRemoteEvent(player, "OnItemAdded")
    EndIf
EndEvent

Event ObjectReference.OnHolotapePlay(ObjectReference akSender, ObjectReference akTerminalRef)
    ProcessHolotape(GetPlayedHolotape(akSender))
EndEvent

Event ObjectReference.OnItemAdded(ObjectReference akSender, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If akSender == Game.GetPlayer() && aiItemCount > 0
        ProcessHolotape(akBaseItem as Holotape)
    EndIf
EndEvent

Holotape Function GetPlayedHolotape(ObjectReference akSender)
    If akSender
        Return akSender.GetBaseObject() as Holotape
    EndIf
    Return None
EndFunction

Function ProcessHolotape(Holotape playedTape)
    If playedTape == None || playedTape != TargetTape
        Return
    EndIf

    If Utility.IsInMenuMode()
        Return
    EndIf

    CancelTimer(CooldownTimerID)
    StartTimer(W05_Wayward_MortTapeTutorialCooldown.GetValue(), CooldownTimerID)

    Int i = 0
    Int count = TutorialData.Length
    While i < count
        If !TutorialData[i].bTimerProcessed
            ShowTutorialEntry(i)
        EndIf
        i += 1
    EndWhile
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID != CooldownTimerID
        Return
    EndIf

    Int i = 0
    Int count = TutorialData.Length
    While i < count
        TutorialData[i].bTimerProcessed = False
        i += 1
    EndWhile
EndEvent

Function ShowTutorialEntry(Int aiIndex)
    If aiIndex < 0 || aiIndex >= TutorialData.Length
        Return
    EndIf
    If TutorialData[aiIndex].TargetMessage != None
        TutorialData[aiIndex].TargetMessage.Show()
    EndIf
    TutorialData[aiIndex].bTimerProcessed = True
EndFunction
