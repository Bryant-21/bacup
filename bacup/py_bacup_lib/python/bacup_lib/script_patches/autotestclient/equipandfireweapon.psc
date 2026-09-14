; FO76 Utility.IntToHex() has no Fallout 4 equivalent, so it is reimplemented in pure
; Papyrus here. Limitation: only non-negative FormIDs are formatted exactly (Papyrus
; has no logical shift and integer division truncates toward zero), which covers every
; regular plugin index; a light-plugin FE... id would render as "0x????????".
String Function HexNibble(Int aiValue)
	String[] digits = new String[16]
	digits[0] = "0"
	digits[1] = "1"
	digits[2] = "2"
	digits[3] = "3"
	digits[4] = "4"
	digits[5] = "5"
	digits[6] = "6"
	digits[7] = "7"
	digits[8] = "8"
	digits[9] = "9"
	digits[10] = "A"
	digits[11] = "B"
	digits[12] = "C"
	digits[13] = "D"
	digits[14] = "E"
	digits[15] = "F"
	Return digits[aiValue]
EndFunction

String Function IntToHex(Int aiValue)
	If aiValue < 0
		Return "0x????????"
	EndIf
	String result = ""
	Int remaining = aiValue
	Int i = 0
	While i < 8
		result = HexNibble(remaining - remaining / 16 * 16) + result
		remaining = remaining / 16
		i += 1
	EndWhile
	Return "0x" + result
EndFunction

Function TestNextWeapon(Int formId)
	weapon weaponForm = game.GetFormFromFile(formId, "SeventySix.esm") as weapon
	If weaponForm == None
		testSucceeded = False
		Return
	EndIf

	ammo ammoForm = weaponForm.getAmmo()
	String weaponFormId = Self.IntToHex(weaponForm.GetFormID())
	String ammoFormId = ""
	If ammoForm != None
		ammoFormId = Self.IntToHex(ammoForm.GetFormID())
	EndIf
	utility.Wait(2.0)
	utility.Wait(2.0)
	utility.Wait(2.0)
	utility.Wait(2.0)
	; FO4 ObjectReference.GetItemCount takes only the form.
	Int ammoPreShots = game.GetPlayer().GetItemCount(ammoForm as form)
	Int shots = 0
	While shots < 150
		utility.Wait(0.1)
		shots = shots + 1
	EndWhile
	Int ammoPostShots = game.GetPlayer().GetItemCount(ammoForm as form)
	utility.Wait(2.0)
	utility.Wait(2.0)
	utility.Wait(2.0)
	If ammoPreShots <= ammoPostShots
		testSucceeded = False
	EndIf
EndFunction
