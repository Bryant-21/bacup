Event OnLoad()
	If WeaponToEquip && !Self.IsDead() && !Self.IsEquipped(WeaponToEquip as Form)
		Self.EquipItem(WeaponToEquip as Form, False, True)
	EndIf
EndEvent
