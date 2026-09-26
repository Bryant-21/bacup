GQ_WorkshopAttackScript Function AttackScript()
	Quest owner = Self as Quest
	Return owner as GQ_WorkshopAttackScript
EndFunction

Function Fragment_Stage_0010_Item_00()
	GQ_WorkshopAttackScript attackScript = AttackScript()
	If attackScript != None
		attackScript.BeginPrepare()
	EndIf
EndFunction

Function Fragment_Stage_0015_Item_00()
	GQ_WorkshopAttackScript attackScript = AttackScript()
	If attackScript != None
		attackScript.BeginAttack()
	EndIf
EndFunction

Function Fragment_Stage_0040_Item_00()
	GQ_WorkshopAttackScript attackScript = AttackScript()
	If attackScript != None
		attackScript.ShutdownAttack(True)
	EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
	GQ_WorkshopAttackScript attackScript = AttackScript()
	If attackScript != None
		attackScript.ShutdownAttack(False)
	EndIf
EndFunction
