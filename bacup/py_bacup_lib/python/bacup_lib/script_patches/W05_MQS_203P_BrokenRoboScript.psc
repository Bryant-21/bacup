Event OnActivate(ObjectReference akActionRef)
    Actor player = CurrentPlayer.GetActorRef()
    if player == None || akActionRef != player
        return
    endif

    int selectedBrain = MessageToShow.Show()
    if selectedBrain <= 0
        return
    endif

    player.SetValue(W05_MQS_203P_ChoseDias, 0.0)
    player.SetValue(W05_MQS_203P_ChoseGreg, 0.0)
    player.SetValue(W05_MQS_203P_ChoseGina, 0.0)
    if selectedBrain == 1
        player.SetValue(W05_MQS_203P_ChoseDias, 1.0)
    elseif selectedBrain == 2
        player.SetValue(W05_MQS_203P_ChoseGreg, 1.0)
    elseif selectedBrain == 3
        player.SetValue(W05_MQS_203P_ChoseGina, 1.0)
    else
        return
    endif
    GetOwningQuest().SetStage(StageToSet)
EndEvent
