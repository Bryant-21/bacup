; Rewritten against Fallout 4's vanilla IDCardReaderScript parent, whose member names
; differ from FO76's: IDCardReader_PlayFailureSound() -> NeedsCardFailureSound.Play(),
; LinkedRefToActivate -> myLinkedRefToActivate,
; shouldActivateAsActivatingPlayer -> shouldActivateAsPlayer, and the state names
; StartsRed/red/StartsRedLockdown/redlockdown/green -> Red/Red_Lockdown/Green.
; BEHAVIOUR LOST: FO76 shouldConsumeIDCard has no FO4 counterpart, so an accepted
; ID card is no longer removed from the inventory.

Key Function FindIDCard(Actor activatingActor)
    If activatingActor == None || IDCards == None
        Return None
    EndIf

    Int i = 0
    While i < IDCards.Length
        If IDCards[i] != None && activatingActor.GetItemCount(IDCards[i]) > 0
            Return IDCards[i]
        EndIf
        i += 1
    EndWhile
    Return None
EndFunction

Function ActivateLinkedObject(Actor activatingActor)
    If myLinkedRefToActivate == None
        Return
    EndIf

    ObjectReference linkedObject = GetLinkedRef(myLinkedRefToActivate)
    If linkedObject != None
        If shouldActivateAsPlayer
            linkedObject.Activate(activatingActor)
        Else
            linkedObject.Activate(Self)
        EndIf
    EndIf
EndFunction

Function ProcessIDCardActivation(ObjectReference akActionRef)
    If lock_IDCardReaderActivation
        Return
    EndIf

    Actor activatingActor = akActionRef as Actor
    If activatingActor == None || activatingActor != Game.GetPlayer()
        Return
    EndIf

    lock_IDCardReaderActivation = True
    Key acceptedCard = FindIDCard(activatingActor)
    If acceptedCard == None
        NeedsCardFailureSound.Play(Self)
        IDCardReaderMessageNeedsCard.Show()
    Else
        WaitFor3DLoad()
        PlayAnimationAndWait("SwipeGreen01", "End")
        GoToState("Green")
        ActivateLinkedObject(activatingActor)
        If shouldAutoReset
            PlayAnimation("JumpRed01")
            GoToState("Red")
        EndIf
    EndIf
    lock_IDCardReaderActivation = False
EndFunction

Function ProcessLockdownActivation(ObjectReference akActionRef)
    If lock_IDCardReaderActivation
        Return
    EndIf

    Actor activatingActor = akActionRef as Actor
    If activatingActor == None || activatingActor != Game.GetPlayer()
        Return
    EndIf

    lock_IDCardReaderActivation = True
    LockdownFailureSound.Play(Self)
    If FindIDCard(activatingActor) != None
        IDCardReaderMessageLockdown.Show()
    Else
        IDCardReaderMessageNeedsCard.Show()
    EndIf
    lock_IDCardReaderActivation = False
EndFunction

Event OnActivate(ObjectReference akActionRef)
    String currentState = GetState()
    If currentState == "Red_Lockdown"
        ProcessLockdownActivation(akActionRef)
    ElseIf currentState == "Red" || currentState == ""
        ProcessIDCardActivation(akActionRef)
    EndIf
EndEvent
