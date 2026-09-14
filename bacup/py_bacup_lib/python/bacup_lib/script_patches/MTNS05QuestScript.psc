Function ResetTargets()
    If CurrentTargetData == None
        Return
    EndIf

    Int index = 0
    While index < CurrentTargetData.Length
        Actor targetActor = CurrentTargetData[index].CurrentTargetActor
        If targetActor != None
            targetActor.SetValue(MTNS05_StageValue, 0.0)
            If CurrentTargetsVoice != None
                CurrentTargetsVoice.RemoveRef(targetActor)
            EndIf
            If FleeingCreatures != None
                FleeingCreatures.RemoveRef(targetActor)
            EndIf
        EndIf

        If CurrentTargetData[index].CurrentTargetAlias != None
            CurrentTargetData[index].CurrentTargetAlias.Clear()
        EndIf
        If CurrentTargetData[index].TargetNameSaved != None
            CurrentTargetData[index].TargetNameSaved.Clear()
        EndIf

        SetObjectiveDisplayed(CurrentTargetData[index].ShootCurrentTargetObjective, false)
        SetObjectiveDisplayed(CurrentTargetData[index].CollectDataCurrentTargetObjective, false)
        SetObjectiveDisplayed(CurrentTargetData[index].RangeCurrentTargetObjective, false)
        SetObjectiveCompleted(CurrentTargetData[index].ShootCurrentTargetObjective, false)
        SetObjectiveCompleted(CurrentTargetData[index].CollectDataCurrentTargetObjective, false)
        SetObjectiveCompleted(CurrentTargetData[index].RangeCurrentTargetObjective, false)

        CurrentTargetData[index].CurrentTargetActor = None
        CurrentTargetData[index].CurrentTargetActiveMagicEffect = None
        CurrentTargetData[index].CurrentTargetRace = None
        index += 1
    EndWhile

    If QTArea01 != None
        QTArea01.Clear()
    EndIf
    If QTArea02 != None
        QTArea02.Clear()
    EndIf
    If QTArea03 != None
        QTArea03.Clear()
    EndIf
EndFunction

Function InitializeTargets()
    ResetTargets()
    If CurrentTargetData == None || CurrentTargetData.Length < 3 || CreatureData == None || CreatureData.Length < 3
        Return
    EndIf

    Int firstIndex = Utility.RandomInt(0, CreatureData.Length - 1)
    Int secondIndex = Utility.RandomInt(0, CreatureData.Length - 1)
    While secondIndex == firstIndex
        secondIndex = Utility.RandomInt(0, CreatureData.Length - 1)
    EndWhile
    Int thirdIndex = Utility.RandomInt(0, CreatureData.Length - 1)
    While thirdIndex == firstIndex || thirdIndex == secondIndex
        thirdIndex = Utility.RandomInt(0, CreatureData.Length - 1)
    EndWhile

    AssignTarget(0, CreatureData[firstIndex], QTArea01)
    AssignTarget(1, CreatureData[secondIndex], QTArea02)
    AssignTarget(2, CreatureData[thirdIndex], QTArea03)
EndFunction

Function AssignTarget(Int aiSlot, CreatureDatum akCreature, ReferenceAlias akQuestTarget)
    If aiSlot < 0 || CurrentTargetData == None || aiSlot >= CurrentTargetData.Length
        Return
    EndIf

    CurrentTargetData[aiSlot].CurrentTargetRace = akCreature.TargetRace
    If CurrentTargetData[aiSlot].TargetNameSaved != None && akCreature.TargetName != None
        CurrentTargetData[aiSlot].TargetNameSaved.ForceLocationTo(akCreature.TargetName)
    EndIf

    If akQuestTarget != None && akCreature.LocationMarkerAlias != None
        ObjectReference markerRef = akCreature.LocationMarkerAlias.GetReference()
        If markerRef != None
            akQuestTarget.ForceRefTo(markerRef)
        EndIf
    EndIf

    SetObjectiveDisplayed(CurrentTargetData[aiSlot].ShootCurrentTargetObjective)
EndFunction

Function HandleVoxTarget(Actor akTarget, ActiveMagicEffect akEffect)
    If akTarget == None || GetStage() < 200 || GetStage() >= ObjectivesDoneStage || CurrentTargetData == None
        Return
    EndIf

    Int index = 0
    While index < CurrentTargetData.Length
        If !IsObjectiveCompleted(CurrentTargetData[index].CollectDataCurrentTargetObjective) && CurrentTargetData[index].CurrentTargetActor == None && CurrentTargetData[index].CurrentTargetRace == akTarget.GetRace()
            CurrentTargetData[index].CurrentTargetActor = akTarget
            CurrentTargetData[index].CurrentTargetActiveMagicEffect = akEffect
            If CurrentTargetData[index].CurrentTargetAlias != None
                CurrentTargetData[index].CurrentTargetAlias.ForceRefTo(akTarget)
            EndIf
            If CurrentTargetsVoice != None
                CurrentTargetsVoice.AddRef(akTarget)
            EndIf
            If FleeingCreatures != None
                FleeingCreatures.AddRef(akTarget)
            EndIf

            akTarget.SetValue(MTNS05_StageValue, (index + 1) as Float)
            SetObjectiveCompleted(CurrentTargetData[index].ShootCurrentTargetObjective)
            SetObjectiveDisplayed(CurrentTargetData[index].CollectDataCurrentTargetObjective)
            SetObjectiveDisplayed(CurrentTargetData[index].RangeCurrentTargetObjective, false)

            Float collectionTime = MTNS05_VoxObjectiveTimer.GetValue()
            If collectionTime <= 0.0
                collectionTime = 1.0
            EndIf
            StartTimer(collectionTime, 100 + index)
            Return
        EndIf
        index += 1
    EndWhile
EndFunction

Function ResetTargetAttempt(Int aiSlot)
    If CurrentTargetData == None || aiSlot < 0 || aiSlot >= CurrentTargetData.Length
        Return
    EndIf

    Actor targetActor = CurrentTargetData[aiSlot].CurrentTargetActor
    If targetActor != None
        targetActor.SetValue(MTNS05_StageValue, 0.0)
        If CurrentTargetsVoice != None
            CurrentTargetsVoice.RemoveRef(targetActor)
        EndIf
        If FleeingCreatures != None
            FleeingCreatures.RemoveRef(targetActor)
        EndIf
    EndIf
    If CurrentTargetData[aiSlot].CurrentTargetAlias != None
        CurrentTargetData[aiSlot].CurrentTargetAlias.Clear()
    EndIf

    CurrentTargetData[aiSlot].CurrentTargetActor = None
    CurrentTargetData[aiSlot].CurrentTargetActiveMagicEffect = None
    SetObjectiveDisplayed(CurrentTargetData[aiSlot].CollectDataCurrentTargetObjective, false)
    SetObjectiveDisplayed(CurrentTargetData[aiSlot].RangeCurrentTargetObjective, false)
    SetObjectiveCompleted(CurrentTargetData[aiSlot].ShootCurrentTargetObjective, false)
    SetObjectiveDisplayed(CurrentTargetData[aiSlot].ShootCurrentTargetObjective)
EndFunction

Function CompleteTarget(Int aiSlot)
    If CurrentTargetData == None || aiSlot < 0 || aiSlot >= CurrentTargetData.Length
        Return
    EndIf

    Actor targetActor = CurrentTargetData[aiSlot].CurrentTargetActor
    If targetActor != None
        targetActor.SetValue(MTNS05_StageValue, 0.0)
        If CurrentTargetsVoice != None
            CurrentTargetsVoice.RemoveRef(targetActor)
        EndIf
        If FleeingCreatures != None
            FleeingCreatures.RemoveRef(targetActor)
        EndIf
    EndIf

    SetObjectiveDisplayed(CurrentTargetData[aiSlot].RangeCurrentTargetObjective, false)
    SetObjectiveCompleted(CurrentTargetData[aiSlot].CollectDataCurrentTargetObjective)
    If MTNS05_TransmissionSuccess_Msg != None
        MTNS05_TransmissionSuccess_Msg.Show()
    EndIf

    If CurrentTargetData.Length >= 3 && IsObjectiveCompleted(CurrentTargetData[0].CollectDataCurrentTargetObjective) && IsObjectiveCompleted(CurrentTargetData[1].CollectDataCurrentTargetObjective) && IsObjectiveCompleted(CurrentTargetData[2].CollectDataCurrentTargetObjective)
        SetStage(ObjectivesDoneStage)
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    Int targetIndex = aiTimerID - 100
    If targetIndex < 0 || CurrentTargetData == None || targetIndex >= CurrentTargetData.Length || GetStage() < 200 || GetStage() >= ObjectivesDoneStage
        Return
    EndIf

    Actor targetActor = CurrentTargetData[targetIndex].CurrentTargetActor
    Actor playerRef = currentPlayer.GetActorReference()
    If targetActor == None || playerRef == None || targetActor.IsDead()
        ResetTargetAttempt(targetIndex)
        Return
    EndIf

    Float targetDistance = targetActor.GetDistance(playerRef)
    If IsObjectiveDisplayed(CurrentTargetData[targetIndex].RangeCurrentTargetObjective)
        If targetDistance > fTargetDistance02
            StartTimer(1.0, aiTimerID)
            Return
        EndIf

        SetObjectiveDisplayed(CurrentTargetData[targetIndex].RangeCurrentTargetObjective, false)
        If MTNS05_EnteringRange_Msg != None
            MTNS05_EnteringRange_Msg.Show()
        EndIf
        Float collectionTime = MTNS05_VoxObjectiveTimer.GetValue()
        If collectionTime <= 0.0
            collectionTime = 1.0
        EndIf
        StartTimer(collectionTime, aiTimerID)
        Return
    EndIf

    If targetDistance > fTargetDistance01
        SetObjectiveDisplayed(CurrentTargetData[targetIndex].RangeCurrentTargetObjective)
        If MTNS05_LeavingRange_Msg != None
            MTNS05_LeavingRange_Msg.Show()
        EndIf
        StartTimer(1.0, aiTimerID)
        Return
    EndIf

    CompleteTarget(targetIndex)
EndEvent
