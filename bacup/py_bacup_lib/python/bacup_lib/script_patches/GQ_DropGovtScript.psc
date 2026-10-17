; GQ_DropGovtMaster's loot tracking. FO76 completed per-player GQ_DropGovt instances; in single
; player the master itself completes when the player takes something from the dropped chest.

Bool Function RequestLocalAirDrop(Location akLocation)
    If IsRunning()
        Return False
    EndIf
    Keyword masterKeyword = Game.GetFormFromFile(0x0004FE7E, "SeventySix.esm") as Keyword
    If masterKeyword == None
        Return False
    EndIf
    ; The quest's RadioStationLocation alias needs an event location.
    If akLocation == None
        akLocation = Game.GetFormFromFile(0x0001558C, "SeventySix.esm") as Location
    EndIf
    If IsCompleted() || IsStageDone(SuccessStage) || IsStageDone(FailureStage)
        Reset()
    EndIf
    CancelTimerGameTime(1)
    CancelTimerGameTime(2)
    Bool sent = masterKeyword.SendStoryEventAndWait(akLocation, Game.GetPlayer())
    Debug.Trace("[GQ_DropGovtScript] air drop requested location=" + akLocation + " sent=" + sent + " running=" + IsRunning())
    If sent && IsRunning()
        StartTimerGameTime(1.0, 1)
        Return True
    EndIf
    Return False
EndFunction

; Single instance: a drop that found no marker, or a crate never looted, would otherwise keep the
; master running and refuse every later holotape and Tales air drop.
Event OnTimerGameTime(Int aiTimerID)
    If !IsRunning() || IsStageDone(SuccessStage) || IsStageDone(FailureStage)
        Return
    EndIf
    ObjectReference chest
    If alias_SupplyChest != None
        chest = alias_SupplyChest.GetReference()
    EndIf
    If chest == None
        ; The air-drop master releases its crate within 90 s of starting.
        Stop()
    ElseIf aiTimerID == 1
        StartTimerGameTime(24.0, 2)
    ElseIf chest.Is3DLoaded()
        StartTimerGameTime(1.0, 2)
    Else
        SetStage(FailureStage)
    EndIf
EndEvent

Function WatchSupplyChest(ObjectReference akChest)
    If akChest == None
        Return
    EndIf
    If alias_SupplyChest != None && alias_SupplyChest.GetReference() != akChest
        alias_SupplyChest.ForceRefTo(akChest)
    EndIf
    AddInventoryEventFilter(None)
    RegisterForRemoteEvent(akChest, "OnItemRemoved")
EndFunction

Event ObjectReference.OnItemRemoved(ObjectReference akSender, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
    If akDestContainer == Game.GetPlayer() && IsRunning() && !IsStageDone(SuccessStage)
        SetStage(SuccessStage)
    EndIf
EndEvent
