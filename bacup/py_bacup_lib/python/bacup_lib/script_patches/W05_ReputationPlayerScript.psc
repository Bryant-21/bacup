; @extends ReferenceAlias
; FO76 put this on the player's NPC_ record, which FO4 cannot carry. Tales'
; B21_TFA_qust_PlayerScripts holds it on a PlayerRef alias instead.
; Converted quest rewards and W05_ModRepScript write the reputation actor values directly, and FO4 has
; no actor-value-changed event, so tier state is polled like W05_ReputationQuestScript's Hostile stages.
; Each faction's vendor ActivateChoice perk (W05_*_PlayerVendorInteractChoicePerk) lives on tiers 1-6
; only; nothing else grants Foundation's. Rep_Mod_Murder is 0.0 in FO76, so kills change nothing and
; there is no OnKill handler. Achievement IDs have no FO4 target.
Event OnAliasInit()
	UpdateReputation()
EndEvent

Event OnPlayerLoadGame()
	UpdateReputation()
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == 76051
		UpdateReputation()
	EndIf
EndEvent

Function UpdateReputation()
	CancelTimer(76051)
	Actor playerRef = GetActorReference()
	If playerRef != None
		Int index = 0
		While ReputationFactionData != None && index < ReputationFactionData.Length
			HoldAboveHostile(playerRef, ReputationFactionData[index])
			index += 1
		EndWhile
		UpdateTierPerks(playerRef)
	EndIf
	StartTimer(5.0, 76051)
EndFunction

; A faction may only drop the player into its Hostile tier once its AllowFactionHostileTier_AV is set.
Function HoldAboveHostile(Actor akPlayer, ReputationFactionDatum akFaction)
	If akFaction == None || akFaction.Reputation_AV == None || akFaction.ReputationTierAboveHostile_Global == None
		Return
	EndIf
	If akFaction.AllowFactionHostileTier_AV != None && akPlayer.GetValue(akFaction.AllowFactionHostileTier_AV) > 0.0
		Return
	EndIf
	Float floor = akFaction.ReputationTierAboveHostile_Global.GetValue()
	If akPlayer.GetValue(akFaction.Reputation_AV) < floor
		akPlayer.SetValue(akFaction.Reputation_AV, floor)
	EndIf
EndFunction

Function UpdateTierPerks(Actor akPlayer)
	Int index = 0
	While ReputationTierData != None && index < ReputationTierData.Length
		ReputationTierDatum tier = ReputationTierData[index]
		If tier != None && tier.Reputation_AV != None && tier.TierPerk != None
			If tier.TierPerk == CurrentTierPerk(akPlayer, tier.Reputation_AV)
				If !akPlayer.HasPerk(tier.TierPerk)
					akPlayer.AddPerk(tier.TierPerk)
				EndIf
			ElseIf akPlayer.HasPerk(tier.TierPerk)
				akPlayer.RemovePerk(tier.TierPerk)
			EndIf
		EndIf
		index += 1
	EndWhile
EndFunction

; ReputationTierData lists each faction's tier globals smallest first, so the last one reached wins.
Perk Function CurrentTierPerk(Actor akPlayer, ActorValue akReputation)
	Float reputation = akPlayer.GetValue(akReputation)
	Perk current = None
	Int index = 0
	While index < ReputationTierData.Length
		ReputationTierDatum tier = ReputationTierData[index]
		If tier != None && tier.Reputation_AV == akReputation && tier.ReputationTier_Global != None && reputation >= tier.ReputationTier_Global.GetValue()
			current = tier.TierPerk
		EndIf
		index += 1
	EndWhile
	Return current
EndFunction
