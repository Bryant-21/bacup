Event OnActivate(ObjectReference akActionRef)
    Actor playerRef = Game.GetPlayer()
    If akActionRef != playerRef || InActivateCooldown
        Return
    EndIf

    InActivateCooldown = True
    StartTimer(CooldownTimer, ActivateCooldownID)

    If MessageToDisplay.Show() != MessageConfirmIndex
        Return
    EndIf
    If InTransactCooldown
        If OBJVendingMachineCraneActivateFail != None
            OBJVendingMachineCraneActivateFail.Play(Self)
        EndIf
        Self.Say(W05_MQ_004P_Crane_TransactionFailed, akTarget = playerRef)
        Return
    EndIf
    If playerRef.GetItemCount(W05_MQ_004P_Crane_MegaToken) < 1
        If OBJVendingMachineCraneActivateFail != None
            OBJVendingMachineCraneActivateFail.Play(Self)
        EndIf
        Self.Say(W05_MQ_004P_Crane_InsufficentFunds, akTarget = playerRef)
        Return
    EndIf

    InTransactCooldown = True
    StartTimer(CooldownTimer, TransactionCooldownID)
    playerRef.RemoveItem(W05_MQ_004P_Crane_MegaToken, 1, True)
    playerRef.AddItem(RewardList, 1, False)
    If OBJVendingMachineCraneActivateSuccess != None
        OBJVendingMachineCraneActivateSuccess.Play(Self)
    EndIf
    PlayAnimation(PlayAnim)
    If StageToSetOnAcquireItem > 0 && !W05_MQ_004P_Crane.IsStageDone(StageToSetOnAcquireItem)
        W05_MQ_004P_Crane.SetStage(StageToSetOnAcquireItem)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == ActivateCooldownID
        InActivateCooldown = False
    ElseIf aiTimerID == TransactionCooldownID
        InTransactCooldown = False
    EndIf
EndEvent

State active
    Event OnBeginState(String asOldState)
        If Is3DLoaded()
            PlayAnimation(PlayAnim)
        EndIf
    EndEvent
EndState
