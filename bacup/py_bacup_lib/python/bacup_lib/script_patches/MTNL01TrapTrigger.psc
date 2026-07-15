Event OnTriggerEnter(ObjectReference akActionRef)
  Actor playerRef = Game.GetPlayer()

  if akActionRef != playerRef
    return
  endif

  if playerRef.GetValue(Perception) >= 2.0 && playerRef.GetValue(MTNL01_TrapWarningPerValue) == 0.0
    playerRef.SetValue(MTNL01_TrapWarningPerValue, 1.0)
    playerRef.AddPerk(MTNL01_ExamineTrap_Perk)
    MTNL01_TrapWarningPer_Msg.Show()
  endif
EndEvent
