Function TryToUseStimpak()
	Bool useCooldown = Cooldown > 0.0 && CooldownTimeStamp != None
	Float currentTime = utility.GetCurrentRealTime()
	If !useCooldown || currentTime > playerTarget.GetValue(CooldownTimeStamp)
		Bool success = False
		; FO4 ObjectReference.GetItemCount takes only the form; FO76's second
		; "include equipped/containers" argument has no equivalent and is dropped.
		If playerTarget.GetItemCount(Stimpak as form) > 0
			Self.UseStimpak(Stimpak)
			success = True
		ElseIf playerTarget.GetItemCount(SuperStimpak as form) > 0
			Self.UseStimpak(SuperStimpak)
			success = True
		ElseIf playerTarget.GetItemCount(StimpakDiluted as form) > 0
			Self.UseStimpak(StimpakDiluted)
			success = True
		EndIf
		If success
			; FO76 IsLocalPlayer() -> single-player identity test.
			If ShowVaultBoySwf && playerTarget == Game.GetPlayer()
				Self.ShowVaultboy()
			EndIf
			If useCooldown
				playerTarget.SetValue(CooldownTimeStamp, currentTime + Cooldown)
			EndIf
		EndIf
	Else
		Self.StartTimer(playerTarget.GetValue(CooldownTimeStamp) - currentTime, 0)
	EndIf
EndFunction
