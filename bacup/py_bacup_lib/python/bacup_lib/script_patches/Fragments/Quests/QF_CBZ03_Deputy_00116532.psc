Function Fragment_Stage_0050_Item_00()
    SetObjectiveDisplayed(50)
    B21WatchChief()
EndFunction

Function Fragment_Stage_0100_Item_00()
    CancelTimer(1165320)
    UnregisterForAllEvents()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(100)

    If CBZ03_Pre_MiscQuestObjective
        CBZ03_Pre_MiscQuestObjective.SetStage(100)
    EndIf

    If Alias_MapMarker && Alias_MapMarker.GetReference()
        Alias_MapMarker.GetReference().AddToMap()
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    Stop()
EndFunction

Actor Function B21ChiefActor()
    ReferenceAlias chiefAlias = GetAlias(1) as ReferenceAlias
    If chiefAlias == None
        Return None
    EndIf
    Return chiefAlias.GetActorReference()
EndFunction

Function B21WatchChief()
    Actor chief = B21ChiefActor()
    If chief != None
        RegisterForRemoteEvent(chief, "OnActivate")
    EndIf
    CancelTimer(1165320)
    StartTimer(5.0, 1165320)
EndFunction

Function B21ChiefBriefed()
    If IsRunning() && !IsStageDone(100)
        SetStage(100)
    EndIf
EndFunction

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer()
        B21ChiefBriefed()
    EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 200
        CancelTimer(1165321)
        StartTimer(20.0, 1165321)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 1165320
        If !IsRunning() || IsStageDone(100)
            Return
        EndIf
        Actor chief = B21ChiefActor()
        Actor playerRef = Game.GetPlayer()
        If chief != None
            RegisterForRemoteEvent(chief, "OnActivate")
            ; The chief's briefing topic did not survive conversion, so reaching him
            ; counts as the conversation the objective asks for.
            If playerRef != None && chief.Is3DLoaded() && playerRef.GetDistance(chief) <= 256.0
                B21ChiefBriefed()
                Return
            EndIf
        EndIf
        StartTimer(5.0, 1165320)
    ElseIf aiTimerID == 1165321
        ; Stage 300 is the quest's only Stop(), and FO76 set it from the server-side
        ; broadcast that played the chief's congratulation on stage 200.
        If IsRunning() && IsStageDone(200) && !IsStageDone(300)
            SetStage(300)
        EndIf
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(1165320)
    CancelTimer(1165321)
    UnregisterForAllEvents()
EndEvent
