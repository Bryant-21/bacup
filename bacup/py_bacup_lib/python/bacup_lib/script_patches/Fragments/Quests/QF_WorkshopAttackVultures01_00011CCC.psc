GQ_WorkshopAttackScorchbeastsScript Function AttackScript()
	Quest owner = Self as Quest
	Return owner as GQ_WorkshopAttackScorchbeastsScript
EndFunction

Function Fragment_Stage_0020_Item_00()
	GQ_WorkshopAttackScorchbeastsScript attackScript = AttackScript()
	If attackScript != None
		attackScript.AbortNoBeasts()
	EndIf
EndFunction

Function Fragment_Stage_0030_Item_00()
	GQ_WorkshopAttackScorchbeastsScript attackScript = AttackScript()
	If attackScript != None
		attackScript.BeginAttack()
	EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
	GQ_WorkshopAttackScorchbeastsScript attackScript = AttackScript()
	If attackScript != None
		attackScript.CompleteAttack()
	EndIf
EndFunction
