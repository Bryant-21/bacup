Function Fragment_Stage_0000_Item_00()
	Actor player = GetPlayerActor()
	If player && AV_QuestCount && player.GetValue(AV_QuestCount) >= 16.0 && pCOMP_Quest_Outtro_Full_Astronaut && pCOMP_Quest_Outtro_Full_Astronaut.IsCompleted() && !IsStageDone(9999)
		SetStage(9999)
	EndIf
EndFunction

Function Fragment_Stage_0030_Item_00()
	SetPlayerValueAtLeast(AV_StrengthEnduranceAgility, 2.0)
EndFunction

Function Fragment_Stage_0031_Item_00()
	SetPlayerValueAtLeast(AV_StrengthEnduranceAgility, 1.0)
EndFunction

Function Fragment_Stage_0032_Item_00()
	SetPlayerValueAtLeast(AV_StrengthEnduranceAgility, 1.0)
EndFunction

Function Fragment_Stage_0034_Item_00()
	SetPlayerValueAtLeast(AV_PerceptionIntelligence, 2.0)
EndFunction

Function Fragment_Stage_0037_Item_00()
	SetPlayerValueAtLeast(AV_Charisma, 2.0)
EndFunction

Function Fragment_Stage_0039_Item_00()
	SetPlayerValueAtLeast(AV_Luck, 2.0)
EndFunction

Function Fragment_Stage_0070_Item_00()
	Actor player = GetPlayerActor()
	If !player
		Return
	EndIf
	Bool gaveStimpak = RemoveOneStimpak(player)
	If gaveStimpak && AV_PlayerGave_Stimpak
		player.SetValue(AV_PlayerGave_Stimpak, 1.0)
	EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
	SetPlayerValueAtLeast(AV_PlayerKnows_WhalesongIsFake, 1.0)
EndFunction

Function Fragment_Stage_0505_Item_00()
	SetPlayerValueAtLeast(AV_PlayerKnows_GutsyHolotape, 1.0)
EndFunction

Function Fragment_Stage_2000_Item_00()
	If pCOMP_Quest_Outtro_Full_Astronaut && pCOMP_Quest_Outtro_Full_Astronaut.IsRunning() && !pCOMP_Quest_Outtro_Full_Astronaut.IsStageDone(9100)
		pCOMP_Quest_Outtro_Full_Astronaut.SetStage(9100)
	EndIf
EndFunction

Actor Function GetPlayerActor()
	Actor player = Alias_Player.GetActorReference()
	If !player
		player = Game.GetPlayer()
		If player
			Alias_Player.ForceRefIfEmpty(player)
		EndIf
	EndIf
	Return player
EndFunction

Function SetPlayerValueAtLeast(ActorValue valueToSet, Float requiredValue)
	Actor player = GetPlayerActor()
	If player && valueToSet && player.GetValue(valueToSet) < requiredValue
		player.SetValue(valueToSet, requiredValue)
	EndIf
EndFunction

Bool Function RemoveOneStimpak(Actor player)
	If Stimpak && player.GetItemCount(Stimpak) > 0
		player.RemoveItem(Stimpak, 1, True)
		Return True
	ElseIf DilutedStimpak && player.GetItemCount(DilutedStimpak) > 0
		player.RemoveItem(DilutedStimpak, 1, True)
		Return True
	ElseIf SuperStimpak && player.GetItemCount(SuperStimpak) > 0
		player.RemoveItem(SuperStimpak, 1, True)
		Return True
	EndIf
	Return False
EndFunction
