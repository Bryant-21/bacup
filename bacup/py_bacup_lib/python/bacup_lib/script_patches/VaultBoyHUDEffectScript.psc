Event OnEffectStart(actor akTarget, actor akCaster)
	Actor playerToNotify
	Bool useCooldown
	Float currentTime
	; FO76 Actor.IsLocalPlayer() -> single-player identity test.
	If akTarget == Game.GetPlayer()
		playerToNotify = akTarget
		If playerToNotify
			useCooldown = Cooldown > 0.0 && CooldownTimestampAV != None
			currentTime = utility.GetCurrentRealTime()
			If !useCooldown || currentTime > playerToNotify.GetValue(CooldownTimestampAV)
				game.ShowPerkVaultBoyOnHUD(VaultBoySwfName, VaultBoySound)
				If useCooldown
					playerToNotify.SetValue(CooldownTimestampAV, currentTime + Cooldown)
				EndIf
			EndIf
		EndIf
	EndIf
EndEvent
