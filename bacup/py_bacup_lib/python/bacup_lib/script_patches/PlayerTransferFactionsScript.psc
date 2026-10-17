; @extends ReferenceAlias
; FO76 put this on the player's NPC_ record, which FO4 cannot carry. Tales'
; B21_TFA_qust_PlayerScripts holds it on a PlayerRef alias instead.
; FO76 re-applied these factions on every server connect because membership did not travel between
; worlds. FO4 saves membership, so the check only has to catch the moment a requirement is first met:
; quest completions and actor values change outside this script, and a location change is the first
; point after either where the faction can matter.
Event OnAliasInit()
	TransferFactions()
EndEvent

Event OnPlayerLoadGame()
	TransferFactions()
EndEvent

Event OnLocationChange(Location akOldLoc, Location akNewLoc)
	TransferFactions()
EndEvent

Function TransferFactions()
	Actor playerRef = GetActorReference()
	If playerRef == None || TransferableFactionRequirementData == None
		Return
	EndIf
	Int index = 0
	While index < TransferableFactionRequirementData.Length
		TransferableFactionRequirement requirement = TransferableFactionRequirementData[index]
		If requirement != None && requirement.FactionToAddPlayerTo != None && !playerRef.IsInFaction(requirement.FactionToAddPlayerTo) && RequirementMet(playerRef, requirement)
			playerRef.AddToFaction(requirement.FactionToAddPlayerTo)
		EndIf
		index += 1
	EndWhile
EndFunction

Bool Function RequirementMet(Actor akPlayer, TransferableFactionRequirement akRequirement)
	If akRequirement.QuestToCheck != None && akRequirement.QuestToCheck.IsCompleted()
		Return True
	EndIf
	Return akRequirement.ActorValueToCheck != None && akPlayer.GetValue(akRequirement.ActorValueToCheck) >= akRequirement.ActorValueRequirement
EndFunction
