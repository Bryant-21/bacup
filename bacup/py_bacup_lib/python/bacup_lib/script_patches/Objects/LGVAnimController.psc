; TODO

Function TurnOff(Actor akSendingPlayer)
    If akSendingPlayer == Game.GetPlayer() && akSendingPlayer == UsingPlayer
        UsingPlayer = None
        GoToState("Off")
    EndIf
EndFunction

State on
    Event OnBeginState(String asOldState)
        PlayAnimation(JumpToOn)
    EndEvent

    Event OnMenuOpenCloseEvent(String asMenuName, Bool abOpening)
    EndEvent
EndState

State Off
    Event OnActivate(ObjectReference akActivator)
        If akActivator == Game.GetPlayer()
            UsingPlayer = akActivator as Actor
        EndIf
    EndEvent
EndState
