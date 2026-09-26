Bool Function IsEventOver()
    If IsStopping() || IsStopped() || IsCompleted()
        Return True
    EndIf
    Return IsStageDone(999)
EndFunction

Actor Function MaskWearerActor()
    If MaskWearer == None
        Return None
    EndIf
    Return MaskWearer.GetActorReference()
EndFunction

ObjectReference Function MaskReference()
    If Mask == None
        Return None
    EndIf
    Return Mask.GetReference()
EndFunction

DefaultQuestObjectiveScript Function RoundObjectives()
    If DefaultQuestObjectiveIns == None
        Quest owner = Self as Quest
        DefaultQuestObjectiveIns = owner as DefaultQuestObjectiveScript
    EndIf
    Return DefaultQuestObjectiveIns
EndFunction

CB02_RankedRewardQuestScript Function RankedRewards()
    Quest owner = Self as Quest
    Return owner as CB02_RankedRewardQuestScript
EndFunction

Function PublishMaskCandy()
    Quest owner = Self as Quest
    B21:QuestVariables variables = owner as B21:QuestVariables
    If variables != None && TextVarMaskCandy != ""
        variables.SetVariable(TextVarMaskCandy, CandyCurrentPool as Float)
    EndIf
EndFunction

Function PlayCandySound(Sound akSound, ObjectReference akSource)
    If akSound != None && akSource != None
        akSound.Play(akSource)
    EndIf
EndFunction

Function PlayConfetti(ObjectReference akSource, Int aiSmallBursts, Int aiMediumBursts)
    If akSource == None
        Return
    EndIf
    Int burst = 0
    While burst < aiSmallBursts && CB02_ExplosionConfetti_Small != None
        akSource.PlaceAtMe(CB02_ExplosionConfetti_Small, 1, False, False, True)
        burst += 1
    EndWhile
    burst = 0
    While burst < aiMediumBursts && CB02_ExplosionConfetti_Medium != None
        akSource.PlaceAtMe(CB02_ExplosionConfetti_Medium, 1, False, False, True)
        burst += 1
    EndWhile
EndFunction

Function ShowCandyFeedback(Actor akRecipient, Int aiAmount, Bool abMaskWearer)
    If akRecipient == None || aiAmount <= 0
        Return
    EndIf
    ; The PlayerCandyGain_* values are the medium and large feedback thresholds; the
    ; mask wearer banks much larger amounts, so it has its own pair.
    Int mediumThreshold = PlayerCandyGain_Medium
    Int largeThreshold = PlayerCandyGain_Large
    If abMaskWearer
        mediumThreshold = PlayerCandyGain_Medium_MaskWearer
        largeThreshold = PlayerCandyGain_Large_MaskWearer
    EndIf

    Sound feedbackSound = QSTCB02CandyPositiveLight
    Topic feedbackTopic = CB02_PlayerGainCandy_Small
    If largeThreshold > 0 && aiAmount >= largeThreshold
        feedbackSound = QSTCB02CandyPositiveHeavy
        feedbackTopic = CB02_PlayerGainCandy_Large
    ElseIf mediumThreshold > 0 && aiAmount >= mediumThreshold
        feedbackSound = QSTCB02CandyPositiveMedium
        feedbackTopic = CB02_PlayerGainCandy_Medium
    EndIf
    PlayCandySound(feedbackSound, akRecipient)
    If abMaskWearer && CB02_MaskGainCandy != None
        akRecipient.Say(CB02_MaskGainCandy)
    ElseIf feedbackTopic != None
        akRecipient.Say(feedbackTopic)
    EndIf
EndFunction

Function GrantCandy(Actor akRecipient, Int aiAmount, Bool abMaskWearer)
    If akRecipient == None || aiAmount <= 0
        Return
    EndIf
    If CB02_CandyGrabbed_AV != None
        akRecipient.ModValue(CB02_CandyGrabbed_AV, aiAmount as Float)
    EndIf
    If CB02_HalloweenCandy != None
        akRecipient.AddItem(CB02_HalloweenCandy, aiAmount, True)
    EndIf
    ShowCandyFeedback(akRecipient, aiAmount, abMaskWearer)
EndFunction

Int Function CandyStealForWeapon(Form akSource, ObjectReference akVictim)
    Int stolen = 1
    Int smallBursts = 1
    Int mediumBursts = 0
    Int row = 0
    While CandyWeaponTypeData != None && row < CandyWeaponTypeData.Length
        CandyWeaponTypeDatum weaponRow = CandyWeaponTypeData[row]
        If akSource != None && weaponRow.WeaponTypeKeyword != None && akSource.HasKeyword(weaponRow.WeaponTypeKeyword)
            Int minimum = weaponRow.CandyStealMin
            Int maximum = weaponRow.CandyStealMax
            If maximum < minimum
                maximum = minimum
            EndIf
            If minimum > 0
                stolen = Utility.RandomInt(minimum, maximum)
            EndIf
            smallBursts = weaponRow.NumberOfSmallConfettiExplosionsToPlay
            mediumBursts = weaponRow.NumberOfMediumConfettiExplosionsToPlay
            row = CandyWeaponTypeData.Length
        Else
            row += 1
        EndIf
    EndWhile
    PlayConfetti(akVictim, smallBursts, mediumBursts)
    Return stolen
EndFunction

Function AnnounceCandyBucket(Bool abAdded)
    If abAdded
        If CB02_CandyBucketAdded_Msg != None
            CB02_CandyBucketAdded_Msg.Show()
        EndIf
    ElseIf CB02_CandyBucketRemoved_Msg != None
        CB02_CandyBucketRemoved_Msg.Show()
    EndIf
EndFunction

Function RefillMaskFromBucket()
    If IsEventOver()
        Return
    EndIf
    ; One active player, so the per-player range is the range.
    Int minimum = BucketCandyMinPerActivePlayer
    Int maximum = BucketCandyMaxPerActivePlayer
    If minimum < BucketCandyMinAbsolute
        minimum = BucketCandyMinAbsolute
    EndIf
    If maximum > BucketCandyMaxAbsolute
        maximum = BucketCandyMaxAbsolute
    EndIf
    If maximum < minimum
        maximum = minimum
    EndIf

    CandyCurrentPool = CandyCurrentPool + Utility.RandomInt(minimum, maximum)
    If CandyCurrentPool > BucketCandyMaxAbsolute
        CandyCurrentPool = BucketCandyMaxAbsolute
    EndIf
    PublishMaskCandy()
    PlayCandySound(QSTCB02CandyReplenish, Game.GetPlayer())
EndFunction

ObjectReference Function PickMaskMarker()
    If MaskSpawnMarkers != None && MaskSpawnMarkers.GetCount() > 0
        Return MaskSpawnMarkers.GetAt(Utility.RandomInt(0, MaskSpawnMarkers.GetCount() - 1))
    EndIf
    Quest owner = Self as Quest
    DefaultEventQuest eventQuest = owner as DefaultEventQuest
    If eventQuest != None && eventQuest.CenterMarker != None
        Return eventQuest.CenterMarker.GetReference()
    EndIf
    Return None
EndFunction

Function RemoveLeftoverMask()
    ObjectReference maskRef = MaskReference()
    If Mask != None
        Mask.Clear()
    EndIf
    If maskRef != None
        maskRef.DisableNoWait()
        maskRef.Delete()
    EndIf
EndFunction

Function PlaceMask()
    If IsEventOver() || CB02_Mask == None
        Return
    EndIf
    ObjectReference marker = PickMaskMarker()
    If marker == None
        StartTimer(TimerDuration_MaskOffRespawn, TimerID_MaskOffRespawn)
        Return
    EndIf

    RemoveLeftoverMask()
    ObjectReference maskRef = marker.PlaceAtMe(CB02_Mask, 1, True, False, False)
    If maskRef == None
        StartTimer(TimerDuration_MaskOffRespawn, TimerID_MaskOffRespawn)
        Return
    EndIf
    If Mask != None
        Mask.ForceRefTo(maskRef)
    EndIf
    SetObjectiveCompleted(GetMaskObjective, False)
    SetObjectiveDisplayed(GetMaskObjective, True, True)
EndFunction

Function TakeMask(Actor akActor)
    If akActor == None || IsEventOver()
        Return
    EndIf
    If MaskWearer != None
        MaskWearer.ForceRefTo(akActor)
    EndIf
    If CB02_PlayerTookMask_Msg != None
        CB02_PlayerTookMask_Msg.Show()
    EndIf
    SetObjectiveCompleted(GetMaskObjective, True)
    SetObjectiveDisplayed(MaskCandyObjective, True, True)
    PublishMaskCandy()
    Float depositDelay = CandyDepositFrequency as Float
    If depositDelay < 1.0
        depositDelay = 1.0
    EndIf
    CancelTimer(TimerID_MaskOffRespawn)
    StartTimer(depositDelay, 24422)
EndFunction

Function DropMask(Bool abRespawn)
    CancelTimer(24422)
    Actor previousWearer = MaskWearerActor()
    If MaskWearer != None
        MaskWearer.Clear()
    EndIf
    If previousWearer != None && CB02_Mask != None
        previousWearer.RemoveItem(CB02_Mask, previousWearer.GetItemCount(CB02_Mask), True)
    EndIf
    RemoveLeftoverMask()
    If CB02_PlayerDroppedMask_Msg != None
        CB02_PlayerDroppedMask_Msg.Show()
    EndIf
    If abRespawn && !IsEventOver()
        ; FO76 reset the mask to another spot in the school for the next contender.
        StartTimer(TimerDuration_MaskOffRespawn, TimerID_MaskOffRespawn)
    EndIf
EndFunction

Function StartEventScene(Scene akScene)
    If akScene != None && !akScene.IsPlaying()
        akScene.Start()
    EndIf
EndFunction

Function BeginRound()
    If IsEventOver()
        Return
    EndIf
    WaitingForMonsterMash = False
    StartMonsterMash = True
    DoOnce_InitialWaitForMonsterMash = True
    CancelTimer(24421)

    If IsObjectiveDisplayed(WaitObjective) && !IsObjectiveCompleted(WaitObjective)
        SetObjectiveCompleted(WaitObjective, True)
    EndIf
    SetObjectiveDisplayed(SpendCandyObjective, True)
    CandyCurrentPool = BucketCandyMinAbsolute
    PublishMaskCandy()
    If MaskWearerActor() == None
        PlaceMask()
    Else
        ; Whoever ends a round wearing the mask keeps it into the next one.
        SetObjectiveDisplayed(MaskCandyObjective, True)
    EndIf
    StartEventScene(CB02_MonsterMash_PASystem)

    DefaultQuestObjectiveScript objectives = RoundObjectives()
    If objectives != None
        objectives.StartObjective(TimeRemainingInRoundObjective)
    EndIf
EndFunction

Function HandleRoundEnd()
    Actor wearer = MaskWearerActor()
    If wearer != None && CandyCurrentPool > 0
        Int payout = CandyCurrentPool
        CandyCurrentPool = 0
        PublishMaskCandy()
        GrantCandy(wearer, payout, True)
    EndIf
    CB02_RankedRewardQuestScript rankedRewards = RankedRewards()
    If rankedRewards != None
        rankedRewards.UpdateRankedPlayers()
    EndIf

    If CurrentRound < MaxRounds
        CurrentRound = CurrentRound + 1
        If EventStageDisableLateJoin >= 0 && !IsStageDone(EventStageDisableLateJoin)
            SetStage(EventStageDisableLateJoin)
        EndIf
        SetStage(10)
        Return
    EndIf
    If !IsStageDone(900)
        SetStage(900)
    EndIf
EndFunction

Function FinishEvent()
    DefaultQuestObjectiveScript objectives = RoundObjectives()
    If objectives != None
        objectives.StopAllObjectives()
    EndIf
    CB02_RankedRewardQuestScript rankedRewards = RankedRewards()
    If rankedRewards != None
        rankedRewards.UpdateRankedPlayers()
    EndIf
    StartEventScene(CB02_MonsterMash_PASystem_EventEnd)
    StartTimer(10.0, 24423)
EndFunction

Function CleanupEvent()
    CancelTimer(24421)
    CancelTimer(24422)
    CancelTimer(24423)
    CancelTimer(TimerID_MaskOffRespawn)
    DropMask(False)
    CandyCurrentPool = 0
    PublishMaskCandy()
    StartMonsterMash = False
    WaitingForMonsterMash = False

    DefaultQuestObjectiveScript objectives = RoundObjectives()
    If objectives != None
        objectives.StopAllObjectives()
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        UnregisterForRemoteEvent(playerRef, "OnItemEquipped")
        UnregisterForRemoteEvent(playerRef, "OnItemUnequipped")
        UnregisterForHitEvent(playerRef)
    EndIf
    If CB02_MonsterMash_PASystem != None && CB02_MonsterMash_PASystem.IsPlaying()
        CB02_MonsterMash_PASystem.Stop()
    EndIf
    If CB02_MaskTeam != None && CB02_MaskTeam.IsRunning()
        CB02_MaskTeam.Stop()
    EndIf
EndFunction

Event OnQuestInit()
    CurrentRound = 1
    CandyCurrentPool = 0
    StartMonsterMash = False
    WaitingForMonsterMash = True
    DoOnce_InitialWaitForMonsterMash = False
    If CB02_MaskTeam != None
        CB02_MaskTeamIns = CB02_MaskTeam as CB02_MaskTeamScript
        If !CB02_MaskTeam.IsRunning()
            CB02_MaskTeam.Start()
        EndIf
    EndIf

    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnItemEquipped")
        RegisterForRemoteEvent(playerRef, "OnItemUnequipped")
        RegisterForHitEvent(playerRef)
    EndIf
    DefaultQuestObjectiveScript objectives = RoundObjectives()
    If objectives != None
        RegisterForCustomEvent(objectives, "ObjectiveEnded")
    EndIf
    PublishMaskCandy()
    ; The PA system explains the rules before the first round starts.
    StartTimer(20.0, 24421)
EndEvent

Event Actor.OnItemEquipped(Actor akSender, Form akBaseObject, ObjectReference akReference)
    If akSender != Game.GetPlayer() || akBaseObject != CB02_Mask || CB02_Mask == None
        Return
    EndIf
    TakeMask(akSender)
EndEvent

Event Actor.OnItemUnequipped(Actor akSender, Form akBaseObject, ObjectReference akReference)
    If akSender != Game.GetPlayer() || akBaseObject != CB02_Mask || CB02_Mask == None
        Return
    EndIf
    If MaskWearerActor() != akSender
        Return
    EndIf
    DropMask(True)
EndEvent

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String apMaterial)
    Actor wearer = MaskWearerActor()
    If wearer == None || akTarget != wearer || abHitBlocked || IsEventOver() || CandyCurrentPool <= 0
        Return
    EndIf
    Int stolen = CandyStealForWeapon(akSource, wearer)
    If stolen > CandyCurrentPool
        stolen = CandyCurrentPool
    EndIf
    If stolen <= 0
        Return
    EndIf
    CandyCurrentPool = CandyCurrentPool - stolen
    PublishMaskCandy()
    PlayCandySound(QSTCB02CandyNegative, wearer)
    If CB02_MaskLoseCandy != None
        wearer.Say(CB02_MaskLoseCandy)
    EndIf
EndEvent

Event DefaultQuestObjectiveScript.ObjectiveEnded(DefaultQuestObjectiveScript akSender, Var[] akArgs)
    If akArgs == None || akArgs.Length < 1 || IsEventOver()
        Return
    EndIf
    If (akArgs[0] as Int) != TimeRemainingInRoundObjective
        Return
    EndIf
    If CompleteRoundStage >= 0
        SetStage(CompleteRoundStage)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 24421
        If !IsEventOver() && !IsStageDone(10)
            SetStage(10)
        EndIf
    ElseIf aiTimerID == 24422
        Actor wearer = MaskWearerActor()
        If wearer == None || IsEventOver()
            Return
        EndIf
        Int payout = (CandyCurrentPool * CandyDepositPercentage) as Int
        If payout < CandyDepositMin
            payout = CandyDepositMin
        EndIf
        If payout > CandyCurrentPool
            payout = CandyCurrentPool
        EndIf
        If payout > 0
            CandyCurrentPool = CandyCurrentPool - payout
            PublishMaskCandy()
            GrantCandy(wearer, payout, True)
        EndIf
        Float depositDelay = CandyDepositFrequency as Float
        If depositDelay < 1.0
            depositDelay = 1.0
        EndIf
        StartTimer(depositDelay, 24422)
    ElseIf aiTimerID == 24423
        If !IsStageDone(999)
            SetStage(999)
        EndIf
    ElseIf aiTimerID == TimerID_MaskOffRespawn
        PlaceMask()
    EndIf
EndEvent

Event OnQuestShutdown()
    CleanupEvent()
EndEvent
