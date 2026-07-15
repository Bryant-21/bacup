Event OnActivate(ObjectReference akActionRef)
    Actor player = PlayerAlias.GetActorRef()
    if player == None || akActionRef != player
        return
    endif

    ObjectReference control = GetRef()
    if control != None
        NPCHumanValveTurn.Play(control)
    endif

    int selectedButton = MessageToShow.Show()
    if selectedButton <= 0
        return
    endif

    if selectedButton == CorrectButton
        player.SetValue(AVToSet, 1.0)
    else
        player.SetValue(AVToSet, 0.0)
    endif
    TryToSetStage()
EndEvent
