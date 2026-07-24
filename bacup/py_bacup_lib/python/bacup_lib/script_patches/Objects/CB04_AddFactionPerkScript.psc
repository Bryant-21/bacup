Event OnEffectStart(Actor akTarget, Actor akCaster)
	if akTarget != None && QuestCompletePerk != None && !akTarget.HasPerk(QuestCompletePerk)
		akTarget.AddPerk(QuestCompletePerk)
	endif
EndEvent
