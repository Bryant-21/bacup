Function StartFiring()
	If isOn
		Return
	EndIf
	InitialAngleX = GetAngleX()
	InitialAngleZ = GetAngleZ()
	isOn = True
	StartTimer(GunTimerDuration(), TimerID)
EndFunction

Function StopFiring()
	isOn = False
	CancelTimer(TimerID)
EndFunction

Float Function GunTimerDuration()
	If TimerDuration != None && TimerDuration.GetValue() > 0.0
		Return TimerDuration.GetValue()
	EndIf
	Return 10.0
EndFunction

Event OnTimer(Int aiTimerID)
	If aiTimerID != TimerID || !isOn
		Return
	EndIf
	If WeaponBase != None && Is3DLoaded()
		Int shots = 1
		If NumProjectiles != None
			shots = NumProjectiles.GetValueInt()
		EndIf
		Int shot = 0
		While shot < shots && isOn
			SetAngle(InitialAngleX + Utility.RandomFloat(-AngleVarianceX, AngleVarianceX), 0.0, InitialAngleZ + Utility.RandomFloat(-AngleVarianceZ, AngleVarianceZ))
			WeaponBase.Fire(Self)
			shot += 1
			Utility.Wait(0.1)
		EndWhile
		SetAngle(InitialAngleX, 0.0, InitialAngleZ)
	EndIf
	If isOn
		StartTimer(GunTimerDuration(), TimerID)
	EndIf
EndEvent
