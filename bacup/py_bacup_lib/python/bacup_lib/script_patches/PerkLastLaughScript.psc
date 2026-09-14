Event OnEffectStart(actor akTarget, actor akCaster)
	; The ScriptObject cast made the compiler resolve OnEnterBleedout on ScriptObject.
	Self.RegisterForRemoteEvent(akTarget, "OnEnterBleedout")
EndEvent

Event actor.OnEnterBleedout(actor akSender)
	; FO76 Actor.GetItemFormsWithKeywords() has no Fallout 4 equivalent -- vanilla FO4
	; cannot enumerate an inventory by keyword. The original scanned every carried
	; grenade type, preferred an equipped one, and otherwise fell back to the first
	; carried type. Only the equipped-grenade path survives here; the "drop a grenade
	; the player is merely carrying" behaviour is lost.
	Actor dyingPlayer = akSender
	If dyingPlayer == None
		Return
	EndIf

	weapon equippedWeapon = dyingPlayer.GetEquippedWeapon()
	If equippedWeapon == None
		Return
	EndIf

	Bool matches = False
	Int i = 0
	While i < WeaponsGrenadesKeywordList.GetSize() && !matches
		keyword grenadeKeyword = WeaponsGrenadesKeywordList.GetAt(i) as keyword
		If grenadeKeyword != None && equippedWeapon.HasKeyword(grenadeKeyword)
			matches = True
		EndIf
		i += 1
	EndWhile

	If !matches
		Return
	EndIf

	Self.DropGrenade(dyingPlayer, equippedWeapon)
	; FO76 IsLocalPlayer() -> single-player identity test.
	If dyingPlayer == Game.GetPlayer()
		Self.ShowVaultboy()
	EndIf
EndEvent
