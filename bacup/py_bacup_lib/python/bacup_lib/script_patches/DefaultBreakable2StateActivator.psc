; FO76 breakable activators are "broken" while in the open state. FO4's
; Default2StateActivator has no SetActivatorOpen / SetActivatorOpenAndWait, so the
; open/closed transition is applied locally, the same way DefaultFixable2StateActivator does.
; OnObjectRepaired / OnObjectDestroyed are FO76-only; FO4 reports the same
; transitions through OnDestructionStageChanged (same mapping as VaultDefaultBreakable2StateActivator).
; @drop-member OnObjectRepaired
; @drop-member OnObjectDestroyed

Event OnDestructionStageChanged(Int aiOldStage, Int aiCurrentStage)
    If aiCurrentStage > 0
        If aiOldStage == 0 && DestructionExplosion != None
            If DestructionExplosionSourceNode != ""
                PlaceAtNode(DestructionExplosionSourceNode, DestructionExplosion)
            Else
                PlaceAtMe(DestructionExplosion)
            EndIf
        EndIf
        ApplyBrokenState(True, False)
    ElseIf aiOldStage > 0
        ApplyBrokenState(False, False)
    EndIf
EndEvent


Bool Function IsActivatorBroken()
    Return isOpen
EndFunction

Event OnLoad()
    ApplyBrokenState(isOpen, True)
EndEvent

Event OnInit()
    ApplyBrokenState(isOpen, True)
EndEvent

Event OnReset()
    ApplyBrokenState(isOpen, True)
EndEvent

Function ApplyBrokenState(Bool abBroken, Bool abSilent)
    Bool wasBroken = isOpen
    isOpen = abBroken
    isAnimating = False
    If abBroken
        myState = 0
        GoToState("Open")
        BlockActivation(False, False)
    Else
        myState = 1
        GoToState("Closed")
        BlockActivation(blockActivationWhenIntact, blockActivationWhenIntact)
    EndIf
    If Is3DLoaded()
        If abBroken
            If abSilent
                PlayAnimation(startOpenAnim)
            Else
                PlayAnimation(openAnim)
            EndIf
        Else
            PlayAnimation(closeAnim)
        EndIf
    EndIf
    If TwoStateCollisionKeyword != None
        If abBroken != InvertCollision
            DisableLinkChain(TwoStateCollisionKeyword)
        Else
            EnableLinkChain(TwoStateCollisionKeyword)
        EndIf
    EndIf
    If wasBroken && !abBroken && !abSilent && FixSound != None
        FixSound.Play(Self)
    EndIf
EndFunction

Function SetBlockActivationWhenIntact(Bool shouldBlockActivationWhenIntact)
    blockActivationWhenIntact = shouldBlockActivationWhenIntact
    If !isOpen
        BlockActivation(blockActivationWhenIntact, blockActivationWhenIntact)
    EndIf
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
