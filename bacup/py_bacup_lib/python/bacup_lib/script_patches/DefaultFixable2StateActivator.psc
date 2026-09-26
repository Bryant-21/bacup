; The native compiler resolves property types on their declaring script.

Bool Function IsActivatorBroken()
    Return (Self as Default2StateActivator).isOpen
EndFunction

Event OnLoad()
    ApplyBrokenState((Self as Default2StateActivator).isOpen, True)
EndEvent

Event OnInit()
    ApplyBrokenState((Self as Default2StateActivator).isOpen, True)
EndEvent

Event OnReset()
    ApplyBrokenState((Self as Default2StateActivator).isOpen, True)
EndEvent

Event OnActivate(ObjectReference akActionRef)
    RepairOnActivation(akActionRef)
EndEvent

Function RepairOnActivation(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer() && (Self as Default2StateActivator).isOpen && !blockActivationWhenBroken
        ApplyBrokenState(False, False)
    EndIf
EndFunction

Function ApplyBrokenState(Bool abBroken, Bool abSilent)
    Bool wasBroken = (Self as Default2StateActivator).isOpen
    (Self as Default2StateActivator).isOpen = abBroken
    (Self as Default2StateActivator).isAnimating = False
    If abBroken
        (Self as Default2StateActivator).myState = 0
        GoToState("Open")
        BlockActivation(blockActivationWhenBroken, blockActivationWhenBroken)
    Else
        (Self as Default2StateActivator).myState = 1
        GoToState("Closed")
        BlockActivation(True, True)
    EndIf
    If Is3DLoaded()
        If abBroken
            If abSilent
                PlayAnimation((Self as Default2StateActivator).startOpenAnim)
            Else
                PlayAnimation((Self as Default2StateActivator).openAnim)
            EndIf
        Else
            PlayAnimation((Self as Default2StateActivator).closeAnim)
        EndIf
    EndIf
    If (Self as Default2StateActivator).TwoStateCollisionKeyword != None
        If abBroken != (Self as Default2StateActivator).InvertCollision
            DisableLinkChain((Self as Default2StateActivator).TwoStateCollisionKeyword)
        Else
            EnableLinkChain((Self as Default2StateActivator).TwoStateCollisionKeyword)
        EndIf
    EndIf
    If wasBroken && !abBroken && !abSilent && FixSound != None
        FixSound.Play(Self)
    EndIf
EndFunction

Function SetBlockActivationWhenBroken(Bool shouldBlockActivationWhenBroken)
    blockActivationWhenBroken = shouldBlockActivationWhenBroken
    If (Self as Default2StateActivator).isOpen
        BlockActivation(blockActivationWhenBroken, blockActivationWhenBroken)
    EndIf
EndFunction

Function SetActivatorOpen(Bool shouldOpen)
    ApplyBrokenState(shouldOpen, False)
EndFunction

Function SetActivatorOpenAndWait(Bool shouldOpen)
    ApplyBrokenState(shouldOpen, False)
EndFunction

Function SetActivatorBroken(Bool isBroken, Bool shouldChangeStateSilently)
    ApplyBrokenState(isBroken, shouldChangeStateSilently)
EndFunction

Function SetActivatorBrokenAndWait(Bool isBroken, Bool shouldChangeStateSilently)
    ApplyBrokenState(isBroken, shouldChangeStateSilently)
EndFunction

State closing
    Function SetActivatorBroken(Bool isBroken, Bool shouldChangeStateSilently)
        ApplyBrokenState(isBroken, shouldChangeStateSilently)
    EndFunction

    Function SetActivatorBrokenAndWait(Bool isBroken, Bool shouldChangeStateSilently)
        ApplyBrokenState(isBroken, shouldChangeStateSilently)
    EndFunction

EndState

State Open
    Function SetActivatorBroken(Bool isBroken, Bool shouldChangeStateSilently)
        ApplyBrokenState(isBroken, shouldChangeStateSilently)
    EndFunction

    Function SetActivatorBrokenAndWait(Bool isBroken, Bool shouldChangeStateSilently)
        ApplyBrokenState(isBroken, shouldChangeStateSilently)
    EndFunction

EndState

State startsclosed
    Function SetActivatorBroken(Bool isBroken, Bool shouldChangeStateSilently)
        ApplyBrokenState(isBroken, shouldChangeStateSilently)
    EndFunction

    Function SetActivatorBrokenAndWait(Bool isBroken, Bool shouldChangeStateSilently)
        ApplyBrokenState(isBroken, shouldChangeStateSilently)
    EndFunction

EndState

State opening
    Function SetActivatorBroken(Bool isBroken, Bool shouldChangeStateSilently)
        ApplyBrokenState(isBroken, shouldChangeStateSilently)
    EndFunction

    Function SetActivatorBrokenAndWait(Bool isBroken, Bool shouldChangeStateSilently)
        ApplyBrokenState(isBroken, shouldChangeStateSilently)
    EndFunction

EndState

State Closed
    Function SetActivatorBroken(Bool isBroken, Bool shouldChangeStateSilently)
        ApplyBrokenState(isBroken, shouldChangeStateSilently)
    EndFunction

    Function SetActivatorBrokenAndWait(Bool isBroken, Bool shouldChangeStateSilently)
        ApplyBrokenState(isBroken, shouldChangeStateSilently)
    EndFunction

EndState

State startsopen
    Function SetActivatorBroken(Bool isBroken, Bool shouldChangeStateSilently)
        ApplyBrokenState(isBroken, shouldChangeStateSilently)
    EndFunction

    Function SetActivatorBrokenAndWait(Bool isBroken, Bool shouldChangeStateSilently)
        ApplyBrokenState(isBroken, shouldChangeStateSilently)
    EndFunction

EndState
