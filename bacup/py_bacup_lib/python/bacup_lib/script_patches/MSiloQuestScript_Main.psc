Event OnQuestInit()
    Initialize()
EndEvent

Function Initialize()
    Quest siloQuest = Self as Quest
    MSiloMain = Self
    MSiloControl = siloQuest as MSiloQuestScript_Control
    MSiloOperations = siloQuest as MSiloQuestScript_Operations
    MSiloStorage = siloQuest as MSiloQuestScript_Storage
    MSiloReactor = siloQuest as MSiloQuestScript_Reactor
    MSiloResidential = siloQuest as MSiloQuestScript_Residential
    MSiloControl.Initialize()
    MSiloOperations.Initialize()
    MSiloStorage.Initialize()
    MSiloReactor.Initialize()
    MSiloResidential.Initialize()
EndFunction

Function SelectLocation(Location akLocation)
    If akLocation == None
        Return
    EndIf

    Int i = 0
    While i < MSiloLocationData.Length
        MSiloLocationDatum locationData = MSiloLocationData[i]
        If locationData.MSiloLocation == akLocation || locationData.MSiloLocation.IsChild(akLocation)
            myMSiloLocationData = locationData
            MSilo_Location.ForceLocationTo(locationData.MSiloLocation)
            MSilo_ExteriorLocation.ForceLocationTo(locationData.MSiloExteriorLocation)
            Return
        EndIf
        i += 1
    EndWhile
EndFunction
