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
    If LinkedRefToActivate == None
        Return
    EndIf

    ObjectReference linkedObject = GetLinkedRef(LinkedRefToActivate)
    If linkedObject != None
        If shouldActivateAsActivatingPlayer
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
        IDCardReader_PlayFailureSound()
        IDCardReaderMessageNeedsCard.Show()
    Else
        If shouldConsumeIDCard
            activatingActor.RemoveItem(acceptedCard, 1, True)
        EndIf
        WaitFor3DLoad()
        PlayAnimationAndWait("SwipeGreen01", "End")
        GoToState("green")
        ActivateLinkedObject(activatingActor)
        If shouldAutoReset
            PlayAnimation("JumpRed01")
            GoToState("red")
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
    IDCardReader_PlayFailureSound()
    If FindIDCard(activatingActor) != None
        IDCardReaderMessageLockdown.Show()
    Else
        IDCardReaderMessageNeedsCard.Show()
    EndIf
    lock_IDCardReaderActivation = False
EndFunction

Event OnActivate(ObjectReference akActionRef)
    String currentState = GetState()
    If currentState == "startsredlockdown" || currentState == "redlockdown"
        ProcessLockdownActivation(akActionRef)
    ElseIf currentState == "StartsRed" || currentState == "red"
        ProcessIDCardActivation(akActionRef)
    EndIf
EndEvent
