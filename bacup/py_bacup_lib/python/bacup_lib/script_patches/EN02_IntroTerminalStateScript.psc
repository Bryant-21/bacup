Event OnActivate(ObjectReference akActionRef)
    Actor playerRef = CurrentPlayer.GetActorRef()
    If akActionRef != playerRef || playerRef == None
        Return
    EndIf
    If playerRef.GetValue(EN02_JoinedEnclaveValue) > 0.0 || playerRef.GetValue(EN02_MODUSIntroOff) > 0.0
        Quest owningQuest = GetOwningQuest()
        If owningQuest != None && iStageToUpdate > 0 && !owningQuest.IsStageDone(iStageToUpdate)
            owningQuest.SetStage(iStageToUpdate)
        EndIf
    EndIf
EndEvent
