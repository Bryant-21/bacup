Function ApplySheepsquatchStage(String asStage, Keyword akStageKeyword)
	If selfRef == None || selfRef.IsDead()
		Return
	EndIf

	String oldStage = GetState()
	If Sheepsquatch_Stage1 != None && Sheepsquatch_Stage1 != akStageKeyword
		selfRef.RemoveKeyword(Sheepsquatch_Stage1)
	EndIf
	If Sheepsquatch_Stage2 != None && Sheepsquatch_Stage2 != akStageKeyword
		selfRef.RemoveKeyword(Sheepsquatch_Stage2)
	EndIf
	If Sheepsquatch_Stage3 != None && Sheepsquatch_Stage3 != akStageKeyword
		selfRef.RemoveKeyword(Sheepsquatch_Stage3)
	EndIf
	If akStageKeyword != None && !selfRef.HasKeyword(akStageKeyword)
		selfRef.AddKeyword(akStageKeyword)
	EndIf
	GoToState(asStage)
	If oldStage != asStage && asStage != "Stage1" && SheepsquatchIdleStateChange != None
		selfRef.PlayIdle(SheepsquatchIdleStateChange)
	EndIf
EndFunction

Function UpdateSheepsquatchStage()
	If selfRef == None || selfRef.IsDead() || Health == None
		Return
	EndIf

	Float healthPercent = selfRef.GetValuePercentage(Health)
	If healthPercent <= HealthPercentageToStage3 && GetState() != "stage3"
		ApplySheepsquatchStage("stage3", Sheepsquatch_Stage3)
	ElseIf healthPercent <= HealthPercentageToStage2 && GetState() == "Stage1"
		ApplySheepsquatchStage("stage2", Sheepsquatch_Stage2)
	EndIf
EndFunction

Event OnEffectStart(Actor akTarget, Actor akCaster)
	selfRef = akCaster
	If selfRef == None
		selfRef = akTarget
	EndIf
	If selfRef == None || selfRef.IsDead()
		Return
	EndIf

	ApplySheepsquatchStage("Stage1", Sheepsquatch_Stage1)
	UpdateSheepsquatchStage()
EndEvent

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String asMaterialName)
	UpdateSheepsquatchStage()
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
	If selfRef != None
		If Sheepsquatch_Stage1 != None
			selfRef.RemoveKeyword(Sheepsquatch_Stage1)
		EndIf
		If Sheepsquatch_Stage2 != None
			selfRef.RemoveKeyword(Sheepsquatch_Stage2)
		EndIf
		If Sheepsquatch_Stage3 != None
			selfRef.RemoveKeyword(Sheepsquatch_Stage3)
		EndIf
	EndIf
	selfRef = None
EndEvent
