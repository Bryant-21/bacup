; Bound to the four malfunction collections. A lure added here shows the
; bound status effect (fire, steam, sparks, or none for lost signal) raised
; by MoveHeight and drops to the bound animation state until re-oriented.
; BrokenSoundMarkerToSpawn and FixedSoundMarkerToSpawn are unbound on every
; collection and stay unused.
Function ENs02_ApplyStatus(ObjectReference akLure)
    If akLure == None
        Return
    EndIf
    If Find(akLure) < 0
        AddRef(akLure)
    EndIf
    ENs02_SetAnimState(akLure, AnimStateToSet)
    If StatusObjectToSpawn != None && SupportObjects != None
        ObjectReference statusFX = akLure.PlaceAtMe(StatusObjectToSpawn, 1, False, False, True)
        If statusFX != None
            statusFX.MoveTo(akLure, 0.0, 0.0, MoveHeight, False)
            If ENs02_StatusLinkKeyword != None
                statusFX.SetLinkedRef(akLure, ENs02_StatusLinkKeyword)
            EndIf
            SupportObjects.AddRef(statusFX)
        EndIf
    EndIf
    ENs02_Say(LineToPlayOnStatusStart)
EndFunction

Function ENs02_ClearStatus(ObjectReference akLure)
    If akLure == None || Find(akLure) < 0
        Return
    EndIf
    RemoveRef(akLure)
    ENs02_RemoveStatusFX(akLure)
    ENs02_Say(LineToPlayOnFix)
EndFunction

Function ENs02_ClearAllStatuses()
    While GetCount() > 0
        ObjectReference lure = GetAt(0)
        RemoveRef(lure)
        ENs02_RemoveStatusFX(lure)
    EndWhile
EndFunction

Function ENs02_RemoveStatusFX(ObjectReference akLure)
    If SupportObjects == None || ENs02_StatusLinkKeyword == None
        Return
    EndIf
    Int index = SupportObjects.GetCount() - 1
    While index >= 0
        ObjectReference statusFX = SupportObjects.GetAt(index)
        If statusFX != None && statusFX.GetLinkedRef(ENs02_StatusLinkKeyword) == akLure
            SupportObjects.RemoveRef(statusFX)
            statusFX.DisableNoWait()
            statusFX.Delete()
        EndIf
        index -= 1
    EndWhile
EndFunction

; Lure Array Component states are bound as OFF, searching, ON.
Function ENs02_SetAnimState(ObjectReference akLure, String asStateName)
    DefaultMultiStateActivator dish = akLure as DefaultMultiStateActivator
    If dish == None
        Return
    EndIf
    Int stateIndex = -1
    If asStateName == "OFF"
        stateIndex = 0
    ElseIf asStateName == "searching"
        stateIndex = 1
    ElseIf asStateName == "ON"
        stateIndex = 2
    EndIf
    If stateIndex >= 0 && dish.CurrentStateIndex != stateIndex
        dish.SetLocalState(stateIndex)
    EndIf
EndFunction

Function ENs02_Say(Topic akTopic)
    EnclaveEventQuestScript eventQuest = GetOwningQuest() as EnclaveEventQuestScript
    If eventQuest != None
        eventQuest.ENEvent_SayToPlayer(akTopic)
    EndIf
EndFunction
