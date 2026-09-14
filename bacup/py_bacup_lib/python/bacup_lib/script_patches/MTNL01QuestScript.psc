Event OnQuestInit()
  ObjectReference keyRef = TrappersKey.GetReference()

  if keyRef != None
    RegisterForRemoteEvent(keyRef, "OnContainerChanged")
  endif
EndEvent

Event ObjectReference.OnContainerChanged(ObjectReference akSender, ObjectReference akNewContainer, ObjectReference akOldContainer)
  if akSender == TrappersKey.GetReference() && akNewContainer == GetTrapPlayer()
    TriggerTrap(akNewContainer as Actor)
  endif
EndEvent

Event OnTimer(int aiTimerID)
  if aiTimerID != TrapWarningId
    return
  endif

  ObjectReference warningSoundRef = TrapSoundMarker.GetReference()
  if warningSoundRef != None
    warningSoundRef.DisableNoWait()
  endif

  ObjectReference explosionMarker = TrappersKeyMarker.GetReference()
  if explosionMarker != None && ExplosionFatMan != None
    explosionMarker.PlaceAtMe(ExplosionFatMan)
  endif
EndEvent

Function DisarmTrap(Actor akOwner)
  if !TrapActive
    return
  endif

  TrapActive = False
  CancelTimer(TrapWarningId)

  ObjectReference warningSoundRef = TrapSoundMarker.GetReference()
  if warningSoundRef != None
    warningSoundRef.DisableNoWait()
  endif

  PulseSoundMarker(TrapDisarmSoundMarker)

  if akOwner != None
    akOwner.RemovePerk(MTNL01_ExamineTrap_Perk)
  endif
EndFunction

Function TriggerTrap(Actor akOwner)
  if !TrapActive
    return
  endif

  TrapActive = False
  PulseSoundMarker(TrapSoundMarker)

  float warningLength = TrapWarningLength as float
  if warningLength <= 0.0
    warningLength = 3.0
  endif

  StartTimer(warningLength, TrapWarningId)

  if akOwner != None
    akOwner.RemovePerk(MTNL01_ExamineTrap_Perk)
  endif
EndFunction

Function PulseSoundMarker(ReferenceAlias akMarker)
  ObjectReference markerRef = akMarker.GetReference()

  if markerRef != None
    markerRef.DisableNoWait()
    markerRef.EnableNoWait()
  endif
EndFunction

Actor Function GetTrapPlayer()
  Actor playerRef = currentPlayer.GetActorReference()

  if playerRef == None
    playerRef = Game.GetPlayer()
  endif

  return playerRef
EndFunction

Function GiveSinglePlayerFinalReward()
  Form chassisReward = Game.GetFormFromFile(0x0051B4DB, "SeventySix.esm")
  if chassisReward != None
    return
  endif

  GiveSinglePlayerBaseGameReward(0x00140C54)
  GiveSinglePlayerBaseGameReward(0x00140C57)
  GiveSinglePlayerBaseGameReward(0x00140C52)
  GiveSinglePlayerBaseGameReward(0x00140C53)
  GiveSinglePlayerBaseGameReward(0x00140C55)
  GiveSinglePlayerBaseGameReward(0x00140C56)
EndFunction

Function GiveSinglePlayerBaseGameReward(Int aiRewardFormID, Int aiCount = 1)
  Actor playerRef = GetTrapPlayer()
  Form rewardForm = Game.GetFormFromFile(aiRewardFormID, "Fallout4.esm")

  if playerRef != None && rewardForm != None
    playerRef.AddItem(rewardForm, aiCount)
  endif
EndFunction

Event OnQuestShutdown()
  CancelTimer(TrapWarningId)

  ObjectReference keyRef = TrappersKey.GetReference()
  if keyRef != None
    UnregisterForRemoteEvent(keyRef, "OnContainerChanged")
  endif

  ObjectReference warningSoundRef = TrapSoundMarker.GetReference()
  if warningSoundRef != None
    warningSoundRef.DisableNoWait()
  endif

  Actor playerRef = GetTrapPlayer()
  if playerRef != None
    playerRef.RemovePerk(MTNL01_ExamineTrap_Perk)
    RemoveQuestItem(playerRef, 0x000504B4)
    RemoveQuestItem(playerRef, 0x000504B8)
    RemoveQuestItem(playerRef, 0x00170490)
    RemoveQuestItem(playerRef, 0x00054316)
    RemoveQuestItem(playerRef, 0x0027E52A)
    RemoveQuestItem(playerRef, 0x002ECCF7)
    RemoveQuestItem(playerRef, 0x002ECD0E)
    RemoveQuestItem(playerRef, 0x002EE9B0)
    RemoveQuestItem(playerRef, 0x004E4B3D)
    RemoveQuestItem(playerRef, 0x0027E58A)
  endif
EndEvent

Function RemoveQuestItem(Actor akPlayer, Int aiItemFormID)
  Form questItem = Game.GetFormFromFile(aiItemFormID, "SeventySix.esm")
  if akPlayer != None && questItem != None
    Int itemCount = akPlayer.GetItemCount(questItem)
    if itemCount > 0
      akPlayer.RemoveItem(questItem, itemCount, True)
    endif
  endif
EndFunction
