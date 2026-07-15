Event OnInit()
    If Game.GetPlayer().GetValue(AVToSet) >= 1.0
        GoToState("active")
        BlockActivation(True, True)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == iFailureID
        bBlockFailure = False
    ElseIf aiTimerID == iConfirmationID
        bBlockConfirmation = False
    EndIf
EndEvent

State Waiting
    Event OnActivate(ObjectReference akActionRef)
        If akActionRef != Game.GetPlayer() || bProcessActivation
            Return
        EndIf

        If akActionRef.GetItemCount(Nuke_LaunchCard) < 1
            If !bBlockFailure
                bBlockFailure = True
                BroadcastFailureSound()
                StartTimer(iAudioCooldown, iFailureID)
            EndIf
            Return
        EndIf

        bProcessActivation = True
        GoToState("active")
        akActionRef.RemoveItem(Nuke_LaunchCard, 1, True)
        Game.GetPlayer().SetValue(AVToSet, 1.0)
        Quest masterQuest = EN07_MQ_Nuke_Master
        If masterQuest != None && !masterQuest.IsRunning()
            masterQuest.Start()
        EndIf
        EN07_NukeMasterScript masterScript = masterQuest as EN07_NukeMasterScript
        If masterScript != None
            masterScript.HandleLocalLaunchCard(Self)
        EndIf
        If PlayAnim != ""
            PlayAnimation(PlayAnim)
        EndIf
        If !bBlockConfirmation
            bBlockConfirmation = True
            BroadcastConfirmationSound()
            StartTimer(iAudioCooldown, iConfirmationID)
        EndIf
        BlockActivation(True, True)
        bProcessActivation = False
    EndEvent
EndState

Function ResetLocalCard()
    Game.GetPlayer().SetValue(AVToSet, 0.0)
    bProcessActivation = False
    bBlockFailure = False
    bBlockConfirmation = False
    GoToState("Waiting")
    BlockActivation(False, False)
EndFunction

State active
    Event OnActivate(ObjectReference akActionRef)
    EndEvent
EndState
