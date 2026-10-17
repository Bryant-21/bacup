Event OnAliasInit()
	OvergrownOnFireFloat = 0.0
	RegisterForHitEvent(Self)
EndEvent

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String apMaterial)
	Quest eventQuest = GetOwningQuest()
	If eventQuest == None || eventQuest.IsStageDone(AllOvergrownBurnedStage)
		Return
	EndIf
	RegisterForHitEvent(Self)
	If eventQuest.IsStageDone(710)
		TryMarkOvergrownBurned(akTarget as Actor, akSource, eventQuest)
	EndIf
EndEvent

Function TryMarkOvergrownBurned(Actor akOvergrown, Form akSource, Quest akEventQuest)
	If akOvergrown == None || Find(akOvergrown) < 0
		Return
	EndIf
	If OvergrownOnFireAV != None && akOvergrown.GetValue(OvergrownOnFireAV) > 0.0
		Return
	EndIf
	Bool burning = akSource != None && WeaponTypeFireDamage != None && akSource.HasKeyword(WeaponTypeFireDamage)
	If !burning && DamageTypeFire != None
		burning = akOvergrown.HasMagicEffectWithKeyword(DamageTypeFire)
	EndIf
	If !burning
		Return
	EndIf
	If OvergrownOnFireAV != None
		akOvergrown.SetValue(OvergrownOnFireAV, 1.0)
	EndIf
	OvergrownOnFireFloat += 1.0
	B21:QuestVariables questVariables = akEventQuest as B21:QuestVariables
	If questVariables != None
		questVariables.SetVariable("Overgrown", OvergrownOnFireFloat)
	EndIf
	If OvergrownOnFireFloat >= FireGoal
		akEventQuest.SetStage(AllOvergrownBurnedStage)
	EndIf
EndFunction
