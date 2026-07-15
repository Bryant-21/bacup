Event OnActivate(ObjectReference akActionRef)
    Actor player = CurrentPlayer.GetActorRef()
    if player == None || akActionRef != player
        return
    endif

    int selectedSetting = W05_MQS_204P_MotherlodeMSG.Show()
    if selectedSetting == 0
        return
    endif

    player.SetValue(W05_MQS_204P_MotherlodeFinalSetting, selectedSetting as float)
    Quest owningQuest = GetOwningQuest()
    if owningQuest != None && StageToSet > 0 && !owningQuest.IsStageDone(StageToSet)
        owningQuest.SetStage(StageToSet)
    endif
EndEvent
