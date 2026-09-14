Function Fragment_Entry_00(objectreference akTargetRef, actor akActor)
	; FO4 AddItem takes (form, count, silent); the extra FO76 arguments are dropped.
	; FO76 ObjectReference.Equip(...) -> FO4 Actor.EquipItem(form, ...), which is the
	; equivalent now that the item lives in the actor inventory.
	akActor.AddItem(akTargetRef as form, 1, True)
	akActor.EquipItem(akTargetRef as form, False, False)
EndFunction

Function Fragment_Entry_01(objectreference akTargetRef, actor akActor)
	akActor.AddItem(akTargetRef as form, 1, True)
	akActor.EquipItem(akTargetRef as form, False, False)
EndFunction
