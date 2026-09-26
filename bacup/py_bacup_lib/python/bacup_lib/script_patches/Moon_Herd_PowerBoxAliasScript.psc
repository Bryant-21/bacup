Event OnAliasInit()
    If StageToSet < 0
        Return
    EndIf
    Int index = 0
    While index < GetCount()
        ObjectReference powerBox = GetAt(index)
        If powerBox != None
            RegisterForRemoteEvent(powerBox, "OnActivate")
        EndIf
        index += 1
    EndWhile
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    Quest owner = GetOwningQuest()
    If StageToSet < 0 || akActionRef != Game.GetPlayer() || owner == None || !owner.IsRunning() || owner.IsStageDone(StageToSet)
        Return
    EndIf
    If preReqStage >= 0 && !owner.IsStageDone(preReqStage)
        Return
    EndIf
    If !IsPowerBoxBroken(akSender)
        Return
    EndIf
    SetPowerBoxFixed(akSender, True)
    Int fixedCount = CountFixedPowerBoxes()
    MOON_Herd_QuestScript eventScript = owner as MOON_Herd_QuestScript
    If eventScript != None
        eventScript.SetTextVariableForCollection(Self as RefCollectionAlias, fixedCount)
    EndIf
    If fixedCount >= GetCount()
        owner.SetStage(StageToSet)
    EndIf
EndEvent

; A fixed breaker keeps its activation blocked, which also marks it on refs whose
; animation script did not convert.
Bool Function IsPowerBoxBroken(ObjectReference akPowerBox)
    If akPowerBox == None
        Return False
    EndIf
    Moon_Herd_ObjectAnimationScript animated = akPowerBox as Moon_Herd_ObjectAnimationScript
    If animated != None
        Return animated.GetState() != "fixed"
    EndIf
    Return !akPowerBox.IsActivationBlocked()
EndFunction

Function SetPowerBoxFixed(ObjectReference akPowerBox, Bool abFixed)
    If akPowerBox == None
        Return
    EndIf
    Moon_Herd_ObjectAnimationScript animated = akPowerBox as Moon_Herd_ObjectAnimationScript
    If animated != None
        If abFixed
            animated.GoToState("fixed")
        Else
            animated.GoToState("broken")
        EndIf
    EndIf
    akPowerBox.BlockActivation(abFixed, abFixed)

    ; The LinkDisable ref is the breaker's objective marker in QT_RefCol_Repellers.
    If LinkDisable != None
        ObjectReference objectiveMarker = akPowerBox.GetLinkedRef(LinkDisable)
        If objectiveMarker != None
            If abFixed
                objectiveMarker.DisableNoWait()
            Else
                objectiveMarker.EnableNoWait()
            EndIf
        EndIf
    EndIf

    If LinkCustom01 != None
        Moon_Herd_ThreeStateLightScript statusLight = akPowerBox.GetLinkedRef(LinkCustom01) as Moon_Herd_ThreeStateLightScript
        If statusLight != None
            If abFixed
                statusLight.GoToState("green")
            Else
                statusLight.GoToState("blinkred")
            EndIf
        EndIf
    EndIf
EndFunction

Int Function CountFixedPowerBoxes()
    Int fixedCount = 0
    Int index = 0
    While index < GetCount()
        ObjectReference powerBox = GetAt(index)
        If powerBox != None && !IsPowerBoxBroken(powerBox)
            fixedCount += 1
        EndIf
        index += 1
    EndWhile
    Return fixedCount
EndFunction

Function SetAllPowerBoxesFixed(Bool abFixed)
    Int index = 0
    While index < GetCount()
        SetPowerBoxFixed(GetAt(index), abFixed)
        index += 1
    EndWhile
EndFunction
