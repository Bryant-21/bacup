GQ_WorkshopAttack_TakeoverScript Function TakeoverScript()
	Quest owner = Self as Quest
	Return owner as GQ_WorkshopAttack_TakeoverScript
EndFunction

Function Fragment_Stage_0010_Item_00()
	GQ_WorkshopAttack_TakeoverScript takeoverScript = TakeoverScript()
	If takeoverScript != None
		takeoverScript.BeginTakeover()
	EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
	GQ_WorkshopAttack_TakeoverScript takeoverScript = TakeoverScript()
	If takeoverScript != None
		takeoverScript.CompleteTakeover()
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	GQ_WorkshopAttack_TakeoverScript takeoverScript = TakeoverScript()
	If takeoverScript != None
		takeoverScript.CleanupTakeover()
	EndIf
EndFunction
