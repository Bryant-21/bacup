Event OnEffectStart(Actor akTarget, Actor akCaster)
	if akCaster != None && akCaster.IsInInterior()
		akCaster.UnequipItem(OutdoorLauncher)
		akCaster.EquipItem(IndoorLauncher)
	endIf
EndEvent
