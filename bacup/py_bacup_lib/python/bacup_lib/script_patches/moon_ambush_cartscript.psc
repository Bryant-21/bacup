Event OnAliasInit()
    brahminRef = GetActorReference()
    B21DepositBusy = False
    ResetCargo()
EndEvent

Event OnAliasShutdown()
    CancelTimer(68318)
EndEvent

Function ResetCargo()
    currentNumberItemsStashed = 0
    PublishCargoCount()
EndFunction

Function PublishCargoCount()
    Quest owner = GetOwningQuest()
    B21:QuestVariables variables = owner as B21:QuestVariables
    If variables != None
        variables.SetVariable("numberOfCargoDeposited", currentNumberItemsStashed as Float)
        variables.SetVariable("maxNumberOfCargo", maxNumberItemsStashed as Float)
    EndIf
EndFunction

; FO76 offered "Deposit Cargo" through BrahminDepositPerk, whose activate choice does
; not survive conversion, so walking up to the brahmin with cargo also loads it.
Function BeginCargoLoading()
    brahminRef = GetActorReference()
    ResetCargo()
    StartTimer(1.0, 68318)
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID != 68318
        Return
    EndIf
    Quest owner = GetOwningQuest()
    If owner == None || !owner.IsRunning() || owner.IsStageDone(stageToSetOnFull)
        Return
    EndIf
    Actor playerRef = Game.GetPlayer()
    ObjectReference brahmin = GetReference()
    If playerRef != None && brahmin != None && itemRequiredToDeposit != None
        If playerRef.GetItemCount(itemRequiredToDeposit) > 0 && playerRef.GetDistance(brahmin) <= 300.0
            DepositCargo(playerRef, False)
        EndIf
    EndIf
    If !owner.IsStageDone(stageToSetOnFull)
        StartTimer(1.0, 68318)
    EndIf
EndEvent

Function PlayInteractionSound(Actor akPlayer, Sound akSound)
    If akSound == None || akPlayer == None
        Return
    EndIf
    If MOON_Ambush_Keyword_SoundCooldown == None || !akPlayer.HasKeyword(MOON_Ambush_Keyword_SoundCooldown)
        akSound.Play(GetReference())
        If MOON_Ambush_SPLL_InteractionSoundCooldown != None
            akPlayer.AddSpell(MOON_Ambush_SPLL_InteractionSoundCooldown, False)
        EndIf
    EndIf
EndFunction

Function PlayDepositTotalFeedback(Actor akPlayer)
    Int index = 0
    While depositTotalsToPlaySound != None && index < depositTotalsToPlaySound.Length
        depositNumberAndSound entry = depositTotalsToPlaySound[index]
        If entry != None && entry.depositNumberToPlaySound == currentNumberItemsStashed
            If entry.soundToPlayOnDeposited != None
                entry.soundToPlayOnDeposited.Play(GetReference())
            EndIf
            If entry.screenShakeToPlayOnSound != None && akPlayer != None
                entry.screenShakeToPlayOnSound.Cast(akPlayer, akPlayer)
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

Event OnActivate(ObjectReference akActionRef)
    Actor playerRef = Game.GetPlayer()
    If akActionRef == playerRef
        DepositCargo(playerRef, True)
    EndIf
EndEvent

; Loading opens once the shack doors are blown (stage 400) and closes when the pack is full.
Function DepositCargo(Actor akPlayer, Bool abReportEmptyHands)
    Quest owner = GetOwningQuest()
    If akPlayer == None || owner == None || !owner.IsRunning() || !owner.IsStageDone(400) || owner.IsStageDone(stageToSetOnFull)
        Return
    EndIf
    If B21DepositBusy
        Return
    EndIf
    B21DepositBusy = True

    Int carried = 0
    If itemRequiredToDeposit != None
        carried = akPlayer.GetItemCount(itemRequiredToDeposit)
    EndIf
    If carried <= 0
        If abReportEmptyHands
            If noCargoToDepositMessage != None
                noCargoToDepositMessage.Show()
            EndIf
            PlayInteractionSound(akPlayer, noCargoToDepositSound)
        EndIf
        B21DepositBusy = False
        Return
    EndIf

    Int amount = carried
    If maxNumberItemsDepositable > 0 && amount > maxNumberItemsDepositable
        amount = maxNumberItemsDepositable
    EndIf
    If amount > maxNumberItemsStashed - currentNumberItemsStashed
        amount = maxNumberItemsStashed - currentNumberItemsStashed
    EndIf
    If amount > 0
        akPlayer.RemoveItem(itemRequiredToDeposit, amount, True)
        currentNumberItemsStashed += amount
        PublishCargoCount()
        If cargoDepositedMessage != None
            cargoDepositedMessage.Show()
        EndIf
        If cargoDepositedSound != None
            cargoDepositedSound.Play(GetReference())
        EndIf
        PlayDepositTotalFeedback(akPlayer)
        If !owner.IsStageDone(500)
            owner.SetStage(500)
        EndIf
        If currentNumberItemsStashed >= maxNumberItemsStashed
            owner.SetStage(stageToSetOnFull)
        EndIf
    EndIf
    B21DepositBusy = False
EndFunction
