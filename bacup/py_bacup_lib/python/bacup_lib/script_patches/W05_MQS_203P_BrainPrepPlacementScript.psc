Event OnActivate(ObjectReference akActionRef)
    Actor player = PlayerAlias.GetActorRef()
    if player == None || akActionRef != player
        return
    endif

    int selectedBrain = MessageToShow.Show()
    if selectedBrain == 1 && player.GetItemCount(W05_MQS_203P_BrainJar_Dias) > 0
        player.RemoveItem(W05_MQS_203P_BrainJar_Dias, 1)
        player.SetValue(W05_MQS_203P_HasDiasBrain, 0.0)
        player.SetValue(W05_MQS_203P_PrepPlacement, 1.0)
    elseif selectedBrain == 2 && player.GetItemCount(W05_MQS_203P_BrainJar_Greg) > 0
        player.RemoveItem(W05_MQS_203P_BrainJar_Greg, 1)
        player.SetValue(W05_MQS_203P_HasGregBrain, 0.0)
        player.SetValue(W05_MQS_203P_PrepPlacement, 2.0)
    elseif selectedBrain == 3 && player.GetItemCount(W05_MQS_203P_BrainJar_Gina) > 0
        player.RemoveItem(W05_MQS_203P_BrainJar_Gina, 1)
        player.SetValue(W05_MQS_203P_HasGinaBrain, 0.0)
        player.SetValue(W05_MQS_203P_PrepPlacement, 3.0)
    endif
EndEvent
