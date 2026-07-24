Event OnActivate(ObjectReference akActionRef)
    LastActivator = akActionRef as Actor
    If GetState() == "opening"
        Return
    EndIf

    Bool turningOn = !IsOpen
    If turningOn && DenyOnPosition
        If CircuitBreakerDeniedMessage
            CircuitBreakerDeniedMessage.Show()
        EndIf
        Return
    EndIf
    If !turningOn && LockToOnPosition
        If CircuitBreakerDeniedMessage
            CircuitBreakerDeniedMessage.Show()
        EndIf
        Return
    EndIf

    If turningOn
        GoToState("opening")
    Else
        GoToState("closed")
    EndIf
EndEvent

State opening
    Event OnBeginState(String asOldState)
        If OpenAnim != "" && Is3DLoaded()
            PlayAnimation(OpenAnim)
        EndIf
        If InvertCollision
            EnableLinkChain(TwoStateCollisionKeyword)
        Else
            DisableLinkChain(TwoStateCollisionKeyword)
        EndIf
        IsOpen = True
        If EnableOverrideActivateText && OverrideActivateTextOpen
            SetActivateTextOverride(OverrideActivateTextOpen)
        EndIf
        GoToState("open")
    EndEvent
EndState

State open
    Event OnBeginState(String asOldState)
        OpenState = 1
    EndEvent
EndState

State closed
    Event OnBeginState(String asOldState)
        If CloseAnim != "" && Is3DLoaded()
            PlayAnimation(CloseAnim)
        EndIf
        If InvertCollision
            DisableLinkChain(TwoStateCollisionKeyword)
        Else
            EnableLinkChain(TwoStateCollisionKeyword)
        EndIf
        IsOpen = False
        OpenState = 0
        If EnableOverrideActivateText && OverrideActivateTextClosed
            SetActivateTextOverride(OverrideActivateTextClosed)
        EndIf
    EndEvent
EndState

Auto State StartsClosed
    Event OnLoad()
        If InvertCollision
            DisableLinkChain(TwoStateCollisionKeyword)
        Else
            EnableLinkChain(TwoStateCollisionKeyword)
        EndIf
        IsOpen = False
        GoToState("closed")
    EndEvent
EndState
