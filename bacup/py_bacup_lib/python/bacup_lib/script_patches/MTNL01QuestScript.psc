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
